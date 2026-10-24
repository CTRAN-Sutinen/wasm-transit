//! End-to-end tests: build a small GTFS zip in memory, parse it, query it.

use std::io::Write;

use chrono::NaiveDate;
use transit_core::Gtfs;

/// Write a tiny but valid GTFS feed: 1 agency, 2 routes, 2 stops,
/// 4 trips (2 weekday block-mates, 1 sunday-only, 1 added by a holiday
/// service exception on a Friday), and date exceptions removing a
/// weekday service and adding the holiday one.
fn fixture_zip() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();

    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).expect("start_file");
        w.write_all(body.as_bytes()).expect("write");
    };

    add(
        &mut w,
        "agency.txt",
        "agency_id,agency_name,agency_url,agency_timezone\n\
         1,Test Transit,https://example.com,America/Montreal\n",
    );
    add(
        &mut w,
        "routes.txt",
        "route_id,agency_id,route_short_name,route_long_name,route_type\n\
         10,1,10,Downtown via Main,3\n\
         20,1,20,Express,3\n",
    );
    add(
        &mut w,
        "stops.txt",
        "stop_id,stop_code,stop_name,stop_lat,stop_lon,location_type,parent_station\n\
         A,,Main & 1st,45.500,-73.500,0,\n\
         B,,Main & 2nd,45.510,-73.510,0,\n",
    );
    add(
        &mut w,
        "trips.txt",
        "route_id,service_id,trip_id,trip_headsign,direction_id,block_id,shape_id\n\
         10,WK,t10,Downtown,0,B1,sh1\n\
         20,WK,t20,Downtown,0,B1,\n\
         10,SUN,t11,Downtown,0,,\n\
         20,HOL,t12,Extra run,0,B2,\n",
    );
    add(
        &mut w,
        "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence,stop_headsign\n\
         t10,08:00:00,08:00:00,A,1,\n\
         t10,08:10:00,08:10:00,B,2,\n\
         t20,08:20:00,08:20:00,A,1,\n\
         t20,08:30:00,08:30:00,B,2,\n\
         t11,09:00:00,09:00:00,A,1,\n\
         t12,07:30:00,07:30:00,B,1,\n",
    );
    add(
        &mut w,
        "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n\
         WK,1,1,1,1,1,0,0,20260101,20261231\n\
         SUN,0,0,0,0,0,0,1,20260101,20261231\n",
    );
    add(
        &mut w,
        "calendar_dates.txt",
        "service_id,date,exception_type\n\
         WK,20261029,2\n\
         HOL,20261023,1\n",
    );
    add(
        &mut w,
        "shapes.txt",
        "shape_id,shape_pt_lat,shape_pt_lon,shape_pt_sequence,shape_dist_traveled\n\
         sh1,45.400,-73.400,1,0.0\n\
         sh1,45.450,-73.450,2,7.0\n\
         sh1,45.500,-73.500,3,14.0\n\
         sh1,45.505,-73.505,4,16.0\n\
         sh1,45.510,-73.510,5,18.0\n",
    );

    w.finish().expect("finish zip").into_inner()
}

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn parses_all_entity_counts() {
    let gtfs = Gtfs::from_zip_bytes(&fixture_zip()).expect("parse");
    assert_eq!(gtfs.agencies.len(), 1);
    assert_eq!(gtfs.routes.len(), 2);
    assert_eq!(gtfs.stops.len(), 2);
    assert_eq!(gtfs.trips.len(), 4);
    assert_eq!(gtfs.stop_times.len(), 6);
    assert_eq!(gtfs.routes["10"].display_name(), "10");
    assert_eq!(gtfs.routes["10"].route_type_name(), "bus");
    assert_eq!(gtfs.stops["A"].stop_name, "Main & 1st");
}

#[test]
fn rejects_feed_without_calendar() {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    // All required files present, but neither calendar.txt nor
    // calendar_dates.txt — the parser must refuse the feed.
    let required = [
        ("agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n"),
        ("routes.txt", "route_id,agency_id,route_short_name,route_type\n1,1,1,3\n"),
        ("stops.txt", "stop_id,stop_name,stop_lat,stop_lon\nA,A,0,0\n"),
        ("trips.txt", "route_id,service_id,trip_id\n1,S,t1\n"),
        ("stop_times.txt", "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt1,08:00:00,08:00:00,A,1\n"),
    ];
    for (name, body) in required {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    }
    let bytes = w.finish().unwrap().into_inner();
    let err = Gtfs::from_zip_bytes(&bytes).expect_err("must fail");
    assert!(err.to_string().contains("calendar"), "unexpected error: {err}");
}

#[test]
fn departures_respect_calendar_and_sort() {
    let gtfs = Gtfs::from_zip_bytes(&fixture_zip()).expect("parse");

    // Friday 2026-10-23: WK runs, SUN does not.
    let deps = gtfs.next_departures("A", date(2026, 10, 23), 0, 10);
    let trip_ids: Vec<&str> = deps.iter().map(|d| d.trip_id.as_str()).collect();
    assert_eq!(trip_ids, vec!["t10", "t20"]); // sorted by time, t11 excluded

    // Sunday 2026-10-25: only SUN service runs.
    let deps = gtfs.next_departures("A", date(2026, 10, 25), 0, 10);
    let trip_ids: Vec<&str> = deps.iter().map(|d| d.trip_id.as_str()).collect();
    assert_eq!(trip_ids, vec!["t11"]);

    // Thursday 2026-10-29: WK removed by calendar_dates exception.
    assert!(gtfs.next_departures("A", date(2026, 10, 29), 0, 10).is_empty());
}

#[test]
fn departures_respect_from_time_limit_and_joins() {
    let gtfs = Gtfs::from_zip_bytes(&fixture_zip()).expect("parse");

    // From 08:01, the 08:00 departure is gone.
    let deps = gtfs.next_departures("A", date(2026, 10, 23), 8 * 3600 + 60, 10);
    assert_eq!(deps.iter().map(|d| d.trip_id.as_str()).collect::<Vec<_>>(), vec!["t20"]);

    // Limit and joined fields.
    let deps = gtfs.next_departures("A", date(2026, 10, 23), 0, 1);
    assert_eq!(deps.len(), 1);
    let d = &deps[0];
    assert_eq!(d.route_name, "10");
    assert_eq!(d.headsign.as_deref(), Some("Downtown"));
    assert_eq!(d.departure_time_hhmmss, "08:00:00");
    assert_eq!(d.departure_time, Some(8 * 3600));

    // Unknown stop: empty, no panic.
    assert!(gtfs.next_departures("ZZZ", date(2026, 10, 23), 0, 10).is_empty());
}

#[test]
fn blocks_list_and_chain_trips() {
    let gtfs = Gtfs::from_zip_bytes(&fixture_zip()).expect("parse");

    // The fixture has two blocks: B1 (t10 + t20) and B2 (the holiday
    // trip t12). t11 has no block_id.
    let blocks = gtfs.blocks();
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].block_id, "B1");
    assert_eq!(blocks[0].trip_count, 2);
    assert_eq!(blocks[1].block_id, "B2");
    assert_eq!(blocks[1].trip_count, 1);

    // Friday: the vehicle runs t10 (08:00 A → 08:10 B), lays over 10
    // minutes, then t20 (08:20 A → 08:30 B).
    let chain = gtfs.block_trips("B1", date(2026, 10, 23));
    let ids: Vec<&str> = chain.iter().map(|t| t.trip_id.as_str()).collect();
    assert_eq!(ids, vec!["t10", "t20"]);

    assert_eq!(chain[0].first_stop_name, "Main & 1st");
    assert_eq!(chain[0].start_time_hhmmss, "08:00:00");
    assert_eq!(chain[0].last_stop_name, "Main & 2nd");
    assert_eq!(chain[0].end_time_hhmmss, "08:10:00");
    assert_eq!(chain[0].layover_secs, None);

    assert_eq!(chain[1].start_time_hhmmss, "08:20:00");
    assert_eq!(chain[1].layover_secs, Some(600));

    // Sunday: WK service is off, so the block has no trips that day.
    assert!(gtfs.block_trips("B1", date(2026, 10, 25)).is_empty());

    // Unknown block: empty, no panic.
    assert!(gtfs.block_trips("ZZZ", date(2026, 10, 23)).is_empty());
}

#[test]
fn departures_expose_block_id() {
    let gtfs = Gtfs::from_zip_bytes(&fixture_zip()).expect("parse");
    let deps = gtfs.next_departures("A", date(2026, 10, 23), 0, 10);
    assert!(deps.iter().all(|d| d.block_id.as_deref() == Some("B1")));
}

#[test]
fn trip_paths_use_shapes_when_available() {
    let gtfs = Gtfs::from_zip_bytes(&fixture_zip()).expect("parse");

    // t10 has shape_id sh1 — the polyline wins (5 points, ending at B).
    assert_eq!(
        gtfs.trip_path("t10"),
        vec![
            (45.400, -73.400),
            (45.450, -73.450),
            (45.500, -73.500),
            (45.505, -73.505),
            (45.510, -73.510),
        ]
    );

    // t20 has no shape — fall back to straight segments between stops.
    assert_eq!(
        gtfs.trip_path("t20"),
        vec![(45.500, -73.500), (45.510, -73.510)]
    );

    // Unknown trip: empty.
    assert!(gtfs.trip_path("nope").is_empty());
}

#[test]
fn trip_path_segments_clip_to_the_ridden_leg() {
    let gtfs = Gtfs::from_zip_bytes(&fixture_zip()).expect("parse");

    // t10 rides A -> B on shape sh1: the segment is the polyline slice
    // between the stops' nearest points, not the whole route.
    assert_eq!(
        gtfs.trip_path_segment("t10", "A", "B"),
        vec![(45.500, -73.500), (45.505, -73.505), (45.510, -73.510)]
    );

    // Reversed direction clips the same slice.
    assert_eq!(
        gtfs.trip_path_segment("t10", "B", "A"),
        vec![(45.500, -73.500), (45.505, -73.505), (45.510, -73.510)]
    );

    // Unshaped trip: stop-to-stop between the two positions.
    assert_eq!(
        gtfs.trip_path_segment("t20", "A", "B"),
        vec![(45.500, -73.500), (45.510, -73.510)]
    );

    // Unknown trip/stop: empty, no panic.
    assert!(gtfs.trip_path_segment("nope", "A", "B").is_empty());
    assert!(gtfs.trip_path_segment("t10", "A", "ZZZ").is_empty());
}

#[test]
fn one_date_can_run_multiple_services() {
    let gtfs = Gtfs::from_zip_bytes(&fixture_zip()).expect("parse");

    // Friday 2026-10-23: WK (weekly pattern) AND HOL (added by a
    // calendar_dates exception) both run.
    assert_eq!(gtfs.active_services(date(2026, 10, 23)), vec!["HOL", "WK"]);
    // A plain Friday with no exception: only WK.
    assert_eq!(gtfs.active_services(date(2026, 10, 30)), vec!["WK"]);
    // Sunday: only SUN.
    assert_eq!(gtfs.active_services(date(2026, 10, 25)), vec!["SUN"]);

    // At stop B on the holiday Friday, departures mix both services,
    // sorted by time: t12 (HOL, 07:30), t10 (WK, 08:10), t20 (WK, 08:30).
    let deps = gtfs.next_departures("B", date(2026, 10, 23), 0, 10);
    let ids: Vec<(&str, &str)> = deps
        .iter()
        .map(|d| (d.trip_id.as_str(), d.service_id.as_str()))
        .collect();
    assert_eq!(ids, vec![("t12", "HOL"), ("t10", "WK"), ("t20", "WK")]);

    // On another Friday, HOL does not run: only the WK trips at B.
    let deps = gtfs.next_departures("B", date(2026, 10, 30), 0, 10);
    let ids: Vec<&str> = deps.iter().map(|d| d.trip_id.as_str()).collect();
    assert_eq!(ids, vec!["t10", "t20"]);

    // Blocks see it too: B2 runs only on the holiday Friday, and its
    // trip carries its service key.
    let chain = gtfs.block_trips("B2", date(2026, 10, 23));
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].trip_id, "t12");
    assert_eq!(chain[0].service_id, "HOL");
    assert!(gtfs.block_trips("B2", date(2026, 10, 30)).is_empty());
}
