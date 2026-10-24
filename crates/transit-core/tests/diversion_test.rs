//! Diversion tests: skip, shift, shape splice, date windows, and the
//! overlay contract (the base feed is never mutated).

use std::io::Write;

use chrono::NaiveDate;
use transit_core::{
    affected_trips, apply_diversions, validate_diversion, Diversion, DetourPoint, Gtfs,
};

/// A three-stop feed: route 10 runs A → B → C twice an hour (t10 with
/// shape sh1, t20 without), all weekday service. Route 20 runs an
/// unrelated A → C express so "other routes untouched" is testable.
fn fixture_zip() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };

    add(
        &mut w,
        "agency.txt",
        "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n",
    );
    add(
        &mut w,
        "routes.txt",
        "route_id,agency_id,route_short_name,route_long_name,route_type\n10,1,10,Main,3\n20,1,20,Express,3\n",
    );
    add(
        &mut w,
        "stops.txt",
        "stop_id,stop_code,stop_name,stop_lat,stop_lon,location_type,parent_station\n\
         A,,Main & 1st,45.500,-73.500,0,\n\
         B,,Main & 2nd,45.510,-73.510,0,\n\
         C,,Main & 3rd,45.520,-73.520,0,\n",
    );
    add(
        &mut w,
        "trips.txt",
        "route_id,service_id,trip_id,trip_headsign,direction_id,block_id,shape_id\n\
         10,WK,t10,Downtown,0,B1,sh1\n\
         10,WK,t20,Downtown,0,B1,\n\
         20,WK,x1,Express,0,,\n",
    );
    add(
        &mut w,
        "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence,stop_headsign\n\
         t10,08:00:00,08:00:00,A,1,\n\
         t10,08:05:00,08:05:00,B,2,\n\
         t10,08:10:00,08:10:00,C,3,\n\
         t20,08:20:00,08:20:00,A,1,\n\
         t20,08:25:00,08:25:00,B,2,\n\
         t20,08:30:00,08:30:00,C,3,\n\
         x1,08:15:00,08:15:00,A,1,\n\
         x1,08:40:00,08:40:00,C,2,\n",
    );
    add(
        &mut w,
        "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n\
         WK,1,1,1,1,1,0,0,20260101,20261231\n",
    );
    add(
        &mut w,
        "shapes.txt",
        "shape_id,shape_pt_lat,shape_pt_lon,shape_pt_sequence,shape_dist_traveled\n\
         sh1,45.400,-73.400,1,0.0\n\
         sh1,45.500,-73.500,2,10.0\n\
         sh1,45.510,-73.510,3,12.0\n\
         sh1,45.520,-73.520,4,14.0\n\
         sh1,45.600,-73.600,5,24.0\n",
    );

    w.finish().unwrap().into_inner()
}

fn friday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()
}

fn diversion(id: &str) -> Diversion {
    Diversion {
        id: id.into(),
        route_id: "10".into(),
        direction_id: None,
        first_kept_stop: "A".into(),
        last_kept_stop: "C".into(),
        skipped_stops: Vec::new(),
        detour: None,
        added_secs: 0,
        start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
    }
}

fn rows(gtfs: &Gtfs, trip_id: &str) -> Vec<(String, u32, Option<u32>)> {
    gtfs.trip_stop_times(trip_id)
        .unwrap()
        .into_iter()
        .map(|r| (r.stop_id, r.stop_sequence, r.departure_time))
        .collect()
}

#[test]
fn validation_rejects_unusable_diversions() {
    let gtfs = Gtfs::from_zip_bytes(&fixture_zip()).unwrap();

    assert!(validate_diversion(&gtfs, &diversion("d1")).is_ok());

    let mut d = diversion("d2");
    d.route_id = "nope".into();
    assert!(validate_diversion(&gtfs, &d).is_err());

    let mut d = diversion("d3");
    d.last_kept_stop = "A".into();
    assert!(validate_diversion(&gtfs, &d).is_err());

    let mut d = diversion("d4");
    d.skipped_stops = vec!["A".into()];
    assert!(validate_diversion(&gtfs, &d).is_err());

    let mut d = diversion("d5");
    d.start_date = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
    d.end_date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    assert!(validate_diversion(&gtfs, &d).is_err());

    let mut d = diversion("d6");
    d.detour = Some(vec![DetourPoint { lat: 1.0, lon: 1.0 }]);
    assert!(validate_diversion(&gtfs, &d).is_err());
}

#[test]
fn affected_trips_matches_route_direction_and_anchors() {
    let gtfs = Gtfs::from_zip_bytes(&fixture_zip()).unwrap();

    // Route 10, both trips have A..C. Route 20's x1 also has A→C but a
    // different route — excluded.
    let mut d = diversion("d1");
    assert_eq!(affected_trips(&gtfs, &d), vec!["t10", "t20"]);

    // Anchors not in order in any trip of the route: no matches.
    d.first_kept_stop = "C".into();
    d.last_kept_stop = "A".into();
    assert!(affected_trips(&gtfs, &d).is_empty());

    // Direction filter.
    let mut d = diversion("d1");
    d.direction_id = Some(1);
    assert!(affected_trips(&gtfs, &d).is_empty());
    d.direction_id = Some(0);
    assert_eq!(affected_trips(&gtfs, &d), vec!["t10", "t20"]);
}

#[test]
fn skipping_a_stop_removes_it_and_renumbers() {
    let base = Gtfs::from_zip_bytes(&fixture_zip()).unwrap();
    let mut d = diversion("d1");
    d.skipped_stops = vec!["B".into()];

    let patched = apply_diversions(&base, friday(), &[d]).unwrap();

    assert_eq!(
        rows(&patched, "t10"),
        vec![
            ("A".into(), 0, Some(8 * 3600)),
            ("C".into(), 1, Some(8 * 3600 + 600)),
        ]
    );
    assert_eq!(
        rows(&patched, "t20"),
        vec![
            ("A".into(), 0, Some(8 * 3600 + 1200)),
            ("C".into(), 1, Some(8 * 3600 + 1800)),
        ]
    );
    // Untouched route and base feed.
    assert_eq!(rows(&patched, "x1"), rows(&base, "x1"));
    assert_eq!(rows(&base, "t10").len(), 3);
}

#[test]
fn added_secs_shift_from_the_last_anchor_onward() {
    let base = Gtfs::from_zip_bytes(&fixture_zip()).unwrap();
    let mut d = diversion("d1");
    d.first_kept_stop = "A".into();
    d.last_kept_stop = "B".into();
    d.added_secs = 300;

    let patched = apply_diversions(&base, friday(), &[d]).unwrap();

    // Boarding at A is on time; B and everything after shift by 5 min.
    assert_eq!(
        rows(&patched, "t10"),
        vec![
            ("A".into(), 0, Some(8 * 3600)),
            ("B".into(), 1, Some(8 * 3600 + 300 + 300)),
            ("C".into(), 2, Some(8 * 3600 + 600 + 300)),
        ]
    );
    // The block chain sees the ripple: t10 now ends at 08:15, and the
    // layover before t20 shrinks from 10 to 5 minutes.
    let chain = patched.block_trips("B1", friday());
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].end_time_hhmmss, "08:15:00");
    assert_eq!(chain[1].layover_secs, Some(300));
}

#[test]
fn detour_splices_the_shape_between_anchors() {
    let base = Gtfs::from_zip_bytes(&fixture_zip()).unwrap();
    let mut d = diversion("d1");
    d.first_kept_stop = "A".into();
    d.last_kept_stop = "C".into();
    d.detour = Some(vec![
        DetourPoint { lat: 45.480, lon: -73.480 },
        DetourPoint { lat: 45.530, lon: -73.530 },
    ]);

    let patched = apply_diversions(&base, friday(), &[d]).unwrap();

    // sh1: nearest A is vertex 1, nearest C is vertex 3. Spliced =
    // [p0, p1, detour..., p3, p4]; the original shape is untouched.
    assert_eq!(
        patched.trip_path("t10"),
        vec![
            (45.400, -73.400),
            (45.500, -73.500),
            (45.480, -73.480),
            (45.530, -73.530),
            (45.520, -73.520),
            (45.600, -73.600),
        ]
    );
    assert_eq!(base.trip_path("t10").len(), 5);
    assert!(patched.trips["t10"].shape_id.as_deref().unwrap().contains("div"));

    // t20 has no shape: the detour applies to schedule only (its path
    // is the stop-to-stop fallback, still A → B → C).
    assert_eq!(patched.trip_path("t20").len(), 3);
}

#[test]
fn date_windows_scoped_per_query_date() {
    let base = Gtfs::from_zip_bytes(&fixture_zip()).unwrap();
    let mut d = diversion("d1");
    d.skipped_stops = vec!["B".into()];
    d.start_date = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
    d.end_date = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();

    // In force on the Friday: B loses its departures.
    let friday_feed = apply_diversions(&base, friday(), &[d.clone()]).unwrap();
    assert!(friday_feed.next_departures("B", friday(), 0, 10).is_empty());

    // Out of force on the Saturday after: base service.
    let saturday = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
    let saturday_feed = apply_diversions(&base, saturday, &[d]).unwrap();
    assert_eq!(rows(&saturday_feed, "t10"), rows(&base, "t10"));
}

#[test]
fn patched_departures_follow_the_new_schedule() {
    let base = Gtfs::from_zip_bytes(&fixture_zip()).unwrap();
    let mut d = diversion("d1");
    d.skipped_stops = vec!["B".into()];
    d.added_secs = 300;

    let patched = apply_diversions(&base, friday(), &[d]).unwrap();

    // C's board: the shifted A→C trips of route 10, plus route 20's
    // express (untouched — different route).
    let ids: Vec<(String, String)> = patched
        .next_departures("C", friday(), 0, 10)
        .into_iter()
        .map(|x| (x.trip_id, x.departure_time_hhmmss))
        .collect();
    assert_eq!(
        ids,
        vec![
            ("t10".into(), "08:15:00".into()),
            ("t20".into(), "08:35:00".into()),
            ("x1".into(), "08:40:00".into()),
        ]
    );
}

#[test]
fn overlapping_diversions_chain() {
    let base = Gtfs::from_zip_bytes(&fixture_zip()).unwrap();
    let mut first = diversion("d1");
    first.added_secs = 300;
    let mut second = diversion("d2");
    second.added_secs = 60;
    second.first_kept_stop = "B".into();
    second.last_kept_stop = "C".into();

    // Applied in list order. d1 anchors A→C: its +300 s lands on C.
    // d2 anchors B→C: its +60 s lands on C too, on top of d1's shift.
    let patched = apply_diversions(&base, friday(), &[first, second.clone()]).unwrap();
    assert_eq!(
        rows(&patched, "t10"),
        vec![
            ("A".into(), 0, Some(8 * 3600)),
            ("B".into(), 1, Some(8 * 3600 + 300)),
            ("C".into(), 2, Some(8 * 3600 + 600 + 300 + 60)),
        ]
    );

    // d2 alone shifts only from its own last anchor (C), not B.
    let alone = apply_diversions(&base, friday(), &[second]).unwrap();
    assert_eq!(
        rows(&alone, "t10"),
        vec![
            ("A".into(), 0, Some(8 * 3600)),
            ("B".into(), 1, Some(8 * 3600 + 300)),
            ("C".into(), 2, Some(8 * 3600 + 600 + 60)),
        ]
    );
}
