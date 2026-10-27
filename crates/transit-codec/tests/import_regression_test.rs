//! Import regression tests: the shapes found importing REAL agency
//! feeds (C-TRAN) that the inference must handle — production trip
//! ids that do not sort by start time, duplicated starts, per-trip
//! run-time variation inside one pattern, impossible block chains,
//! aspirational transfers — plus the rider-view and alignment laws
//! of the round trip. Every test builds its feed inline; import is
//! the codec under test, so these live at the codec's own seam.

use std::io::Write;

use transit_codec::{gtfs_to_network, network_to_gtfs};
use transit_network::Departures;

const DATE: &str = "2026-10-23"; // Friday: WK + HOL both run

/// The e2e fixture feed (see web/e2e/make_fixtures.py): stops A, B, C;
/// t10 A 08:00 -> B 08:10 (shape), t20 A 08:20 -> B 08:30, t30
/// A 08:40 -> C 08:45 -> B 08:50; SUN and HOL variants.
pub fn fixture_zip() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };

    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n20,1,20,3\n");
    add(
        &mut w,
        "stops.txt",
        "stop_id,stop_name,stop_lat,stop_lon\nA,Main & 1st,45.500,-73.500\nB,Main & 2nd,45.510,-73.510\nC,Main & 3rd,45.520,-73.520\n",
    );
    add(
        &mut w,
        "trips.txt",
        "route_id,service_id,trip_id,direction_id,block_id,shape_id\n10,WK,t10,0,B1,sh1\n20,WK,t20,0,B1,\n10,WK,t30,0,,\n10,SUN,t11,0,,\n20,HOL,t12,0,B2,\n",
    );
    add(
        &mut w,
        "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt10,08:00:00,08:00:00,A,1\nt10,08:10:00,08:10:00,B,2\nt20,08:20:00,08:20:00,A,1\nt20,08:30:00,08:30:00,B,2\nt30,08:40:00,08:40:00,A,1\nt30,08:45:00,08:45:00,C,2\nt30,08:50:00,08:50:00,B,3\nt11,09:00:00,09:00:00,A,1\nt11,09:10:00,09:10:00,B,2\nt12,07:30:00,07:30:00,B,1\n",
    );
    add(
        &mut w,
        "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWK,1,1,1,1,1,0,0,20260101,20261231\nSUN,0,0,0,0,0,0,1,20260101,20261231\n",
    );
    add(&mut w, "calendar_dates.txt", "service_id,date,exception_type\nHOL,20261023,1\n");
    add(
        &mut w,
        "shapes.txt",
        "shape_id,shape_pt_lat,shape_pt_lon,shape_pt_sequence\nsh1,45.400,-73.400,1\nsh1,45.500,-73.500,2\nsh1,45.510,-73.510,3\n",
    );
    add(
        &mut w,
        "feed_info.txt",
        "feed_publisher_name,feed_publisher_url,feed_lang,feed_version\nTest Transit,https://example.com,en,2026.10.1\n",
    );
    w.finish().unwrap().into_inner()
}

/// A production-shaped feed: trip ids that do not sort by start
/// time, a duplicated start, and per-trip run-time variation inside
/// one pattern. These shapes are routine in real agency feeds (found
/// importing C-TRAN) and the inference must still produce a valid
/// document — "import is valid by construction" is the contract that
/// lets the planner edit a derived network immediately.
pub fn production_zip() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };

    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n");
    add(
        &mut w,
        "stops.txt",
        "stop_id,stop_name,stop_lat,stop_lon\nA,Main & 1st,45.500,-73.500\nB,Main & 2nd,45.510,-73.510\nC,Main & 3rd,45.520,-73.520\n",
    );
    // Lexical trip-id order is NOT start-time order (t1 < t3 < t5 <
    // t9), t1 and t3 duplicate the 08:00 start, and the pattern's
    // earliest trip (t9) runs 10 minutes slower than the rest.
    add(
        &mut w,
        "trips.txt",
        "route_id,service_id,trip_id,direction_id,block_id\n10,WK,t1,0,B1\n10,WK,t3,0,\n10,WK,t5,0,B1\n10,WK,t9,0,B1\n10,WK,t7,0,\n",
    );
    add(
        &mut w,
        "stop_times.txt",
        // t9 (the pattern's earliest trip) runs A→C in 30 minutes;
        // t1/t3/t5 run it in 20. The block chain is feasible against
        // every trip's own times (t1 arrives 08:20, t5 departs 08:22).
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
         t9,06:00:00,06:00:00,A,1\nt9,06:10:00,06:10:00,B,2\nt9,06:30:00,06:30:00,C,3\n\
         t1,08:00:00,08:00:00,A,1\nt1,08:08:00,08:08:00,B,2\nt1,08:20:00,08:20:00,C,3\n\
         t3,08:00:00,08:00:00,A,1\nt3,08:08:00,08:08:00,B,2\nt3,08:20:00,08:20:00,C,3\n\
         t5,08:22:00,08:22:00,A,1\nt5,08:30:00,08:30:00,B,2\nt5,08:42:00,08:42:00,C,3\n\
         t7,09:00:00,09:00:00,A,1\nt7,09:10:00,09:10:00,B,2\n",
    );
    add(
        &mut w,
        "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWK,1,1,1,1,1,0,0,20260101,20261231\n",
    );
    w.finish().unwrap().into_inner()
}

pub fn import_production() -> transit_network::Network {
    let gtfs = transit_core::Gtfs::from_zip_bytes(&production_zip()).unwrap();
    gtfs_to_network(&gtfs).network
}

#[test]
fn import_infers_a_valid_document_from_a_feed() {
    let gtfs = transit_core::Gtfs::from_zip_bytes(&fixture_zip()).unwrap();
    let network = gtfs_to_network(&gtfs).network;
    // The inference must produce a document the model accepts — that
    // is the contract that lets the planner edit it immediately.
    assert!(network.validate().is_ok(),
        "inferred document must validate: {:?}", transit_network::validation_issues(&network));

    // Fixture shape (see fixture_zip): 3 stops, 2 routes, three
    // two-plus-stop patterns (t12 is a single-stop trip — dropped by
    // design), two calendars (HOL served only t12), four services.
    assert_eq!(network.stops.len(), 3);
    assert_eq!(network.routes.len(), 2);
    assert_eq!(network.patterns.len(), 3);
    assert_eq!(network.service_types.len(), 2);
    assert!(network.service_types.contains_key("WK"));
    assert!(network.service_types.contains_key("SUN"));
    assert_eq!(network.pattern_trips.len(), 4);

    // Route 10's A→B pattern: exact starts 08:00 (WK, t10) and
    // 09:00 (SUN, t11) — one service per (service key, pattern).
    let svc = network.pattern_trips.get("WK:pat-10-0-0").expect("service id scheme");
    assert_eq!(svc.departures, Departures::ExactTimes { starts: vec![8 * 3600] });
    let pattern = &network.patterns[&svc.pattern];
    assert_eq!(pattern.stops.len(), 2);
    assert!(pattern.stops.iter().all(|s| s.timepoint));

    // The alignment survives inference: t10's sh1 (3 points) is the
    // pattern's path; patterns whose trips carry no shapes have none.
    let path = pattern.path.as_ref().expect("sh1 becomes the pattern path");
    assert_eq!(path.len(), 3);
    assert_eq!((path[0].lat, path[0].lon), (45.400, -73.400));
    let unshaped: Vec<&String> = network
        .patterns
        .iter()
        .filter(|(_, p)| p.path.is_none())
        .map(|(id, _)| id)
        .collect();
    assert_eq!(unshaped.len(), 2, "only pat-10-0-0 has a path: {unshaped:?}");

    // The runcut survives: B1 chains t10 and t20 chronologically.
    let block = network.blocks.get("B1").expect("block B1 kept");
    assert_eq!(block.trips.len(), 2);
    assert_eq!(block.trips[0].start_secs, 8 * 3600);
    assert_eq!(block.trips[0].pattern_trips, "WK:pat-10-0-0");
    // GTFS carries no layover data; the import infers each layover
    // from the observed end→start gap (t10 ends 08:10, t20 starts
    // 08:20 — exactly feasible, the model's law holds with
    // equality). The planner re-authors from there.
    assert_eq!(block.layovers.len(), 1, "the observed gap becomes the layover");
    assert_eq!(block.layovers[0].after_trip, 0);
    assert_eq!(block.layovers[0].secs, 600);
}

#[test]
fn an_authored_document_materializes_like_any_feed() {
    // A planner's authored document is a first-class feed: the shared
    // network fixture (testdata/network.json — the manifest sample
    // load reads the same file) expands and reparses with the
    // summary the feed line shows: 3 stops, 1 route, 3 trips,
    // 9 stop_times.
    let net: transit_network::Network =
        serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap();
    let gtfs = network_to_gtfs(&net).expect("fixture network materializes");
    let summary = transit_core::Gtfs::from_zip_bytes(&transit_core::write::to_zip_bytes(&gtfs))
        .unwrap()
        .summary();
    assert_eq!((summary.stops, summary.routes, summary.trips, summary.stop_times), (3, 1, 3, 9));
}

#[test]
fn import_sorts_and_dedupes_exact_starts() {
    // Regression (C-TRAN): members arrive in trip-id order and real
    // feeds carry duplicate starts; the model requires sorted,
    // unique starts. t1/t3 duplicate 08:00 and t9's 06:00 sorts
    // after them in trip-id order.
    let network = import_production();
    let svc = network.pattern_trips.get("WK:pat-10-0-1").expect("service");
    assert_eq!(
        svc.departures,
        Departures::ExactTimes { starts: vec![6 * 3600, 8 * 3600, 8 * 3600 + 22 * 60] }
    );
}

#[test]
fn import_keeps_feasible_blocks_despite_runtime_variation() {
    // Regression (C-TRAN): the timing profile used to come from the
    // pattern's earliest trip; applying that trip's 30-minute
    // runtime to the later 20-minute trips pushed modeled arrivals
    // past the next block trip's start (187 issues on the real
    // feed, every derived network unloadable). The profile must
    // come from the shortest-running member, so connections that
    // hold in the source feed hold in the model.
    let network = import_production();
    assert!(network.validate().is_ok(),
        "inferred document must validate: {:?}", transit_network::validation_issues(&network));
    let block = network.blocks.get("B1").expect("block B1 kept");
    assert_eq!(block.trips.len(), 3);
    // The model's arrival for t1 is start + the 20-minute runtime.
    let t1_arrival = transit_network::trip_arrival_end_secs(&network, "WK:pat-10-0-1", 8 * 3600)
        .expect("t1 arrival");
    assert_eq!(t1_arrival, 8 * 3600 + 20 * 60);
    assert!(t1_arrival <= block.trips[2].start_secs);
    // The whole point: the derived document loads back into the app.
    assert!(network_to_gtfs(&network).is_ok());
}

/// A feed carrying all four resolution shapes: an infeasible block
/// chain (departs before the vehicle arrives), two trips of one
/// block at the same second, the same trip claimed by two blocks,
/// and a single-stop trip. The import must resolve every one — AND
/// report it. Silently dropping is what the notes exist to prevent.
#[test]
fn import_reports_its_resolutions() {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };
    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n");
    add(&mut w, "stops.txt", "stop_id,stop_name,stop_lat,stop_lon\nA,Main & 1st,45.500,-73.500\nB,Main & 2nd,45.510,-73.510\n");
    // t1/t2: block B1, t2 departs 08:05 while t1 arrives 08:10 (the
    // pattern's shortest runtime) — genuinely infeasible source
    // data. t3/t4: block B2, both at 09:00. t5: block B3 claims the
    // same derived trip as t3. t6: a single-stop trip.
    add(&mut w, "trips.txt",
        "route_id,service_id,trip_id,block_id\n10,WK,t1,B1\n10,WK,t2,B1\n10,WK,t3,B2\n10,WK,t4,B2\n10,WK,t5,B3\n10,WK,t6,\n");
    add(&mut w, "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
         t1,08:00:00,08:00:00,A,1\nt1,08:30:00,08:30:00,B,2\n\
         t2,08:05:00,08:05:00,A,1\nt2,08:15:00,08:15:00,B,2\n\
         t3,09:00:00,09:00:00,A,1\nt3,09:10:00,09:10:00,B,2\n\
         t4,09:00:00,09:00:00,B,1\nt4,09:10:00,09:10:00,A,2\n\
         t5,09:00:00,09:00:00,A,1\nt5,09:10:00,09:10:00,B,2\n\
         t6,07:00:00,07:00:00,B,1\n");
    add(&mut w, "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWK,1,1,1,1,1,0,0,20260101,20261231\n");
    let bytes = w.finish().unwrap().into_inner();

    let gtfs = transit_core::Gtfs::from_zip_bytes(&bytes).unwrap();
    let transit_codec::ImportOutcome { network, notes, .. } = gtfs_to_network(&gtfs);
    // The whole point: the resolved document is valid.
    assert!(network.validate().is_ok(),
        "resolved document must validate: {:?}", transit_network::validation_issues(&network));
    // B1 keeps t1 (t2 was an impossible continuation); B2 keeps t3
    // (t4 was the same second); B3 is gone (its only trip already
    // belonged to B2).
    assert_eq!(network.blocks.get("B1").unwrap().trips.len(), 1);
    assert_eq!(network.blocks.get("B2").unwrap().trips.len(), 1);
    assert!(network.blocks.get("B3").is_none());

    // And every resolution is reported — one note per shape.
    assert_eq!(notes.len(), 4, "one note per resolution: {notes:?}");
    let kinds: Vec<String> = notes
        .iter()
        .map(|n| serde_json::to_value(n).unwrap()["kind"].as_str().unwrap().to_string())
        .collect();
    assert!(kinds.contains(&"infeasible_block_chain".to_string()), "{kinds:?}");
    assert!(kinds.contains(&"same_second_block_trips".to_string()), "{kinds:?}");
    assert!(kinds.contains(&"duplicate_block_trip".to_string()), "{kinds:?}");
    assert!(kinds.contains(&"dropped_trips".to_string()), "{kinds:?}");
}

/// Transfer centers across a street: routes 10 and 20 meet at
/// sibling stops N1/N2 (parent station STN), with transfers.txt
/// declaring the connection. Plus a transfers.txt row on a
/// single-route pair (A, C) whose intent cannot compose.
fn transfer_zip() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };
    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n20,1,20,3\n");
    add(&mut w, "stops.txt",
        "stop_id,stop_name,stop_lat,stop_lon,location_type,parent_station\n\
         STN,Transfer Center,45.510,-73.510,1,\n\
         N1,Main & 1st,45.510,-73.5105,0,STN\n\
         N2,Main & 1st across,45.510,-73.5095,0,STN\n\
         A,Elm & 1st,45.500,-73.500,0,\n\
         C,Oak & 3rd,45.520,-73.520,0,\n");
    add(&mut w, "trips.txt", "route_id,service_id,trip_id,direction_id,block_id\n10,WK,t10,0,\n20,WK,t20,0,\n");
    add(&mut w, "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
         t10,08:00:00,08:00:00,A,1\nt10,08:10:00,08:10:00,N1,2\n\
         t20,08:20:00,08:20:00,N2,1\nt20,08:30:00,08:30:00,C,2\n");
    add(&mut w, "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWK,1,1,1,1,1,0,0,20260101,20261231\n");
    // N1↔N2 is a real transfer point (two routes across the street);
    // A↔C is the feed's intent, but nothing composes there.
    add(&mut w, "transfers.txt", "from_stop_id,to_stop_id,transfer_type\nN1,N2,0\nA,C,0\n");
    w.finish().unwrap().into_inner()
}

#[test]
fn import_flags_transfer_points_and_reports_unusable_ones() {
    let gtfs = transit_core::Gtfs::from_zip_bytes(&transfer_zip()).unwrap();
    let transit_codec::ImportOutcome { network, notes, .. } = gtfs_to_network(&gtfs);
    // The composed flags land; the aspirational ones do not — and are
    // reported instead of silently ignored.
    assert!(network.stops["N1"].transfer, "two routes meet across the street");
    assert!(network.stops["N2"].transfer);
    assert!(!network.stops["A"].transfer);
    let unusable: Vec<String> = notes
        .iter()
        .filter_map(|n| match n {
            transit_codec::ImportNote::UnusableTransferPoint { stop } => Some(stop.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(unusable, vec!["A".to_string(), "C".to_string()], "{notes:?}");
    // And the flagged document still validates (import law).
    assert!(network.validate().is_ok(), "{:?}", transit_network::validation_issues(&network));
    // The analysis answers the composed question over the same
    // fixture — pinned at the analysis crate's seam (regression
    // test: the walking group pools both routes and the day gives
    // route 10 → 20 a usable timed connection).
    let date = chrono::NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap();
    let stats = transit_analysis::network_stats(&network, date);
    assert_eq!(stats.transfer_stops, 2);
    let n1 = stats.stops.iter().find(|s| s.stop_id == "N1").expect("N1");
    assert!(n1.transfer);
    assert_eq!(n1.transfer_group, Some(3));
    assert_eq!(n1.transfer_pairs, Some(1), "only 10 → 20 fits the window");
    let a = stats.stops.iter().find(|s| s.stop_id == "A").expect("A");
    assert!(!a.transfer);
    assert_eq!(a.transfer_group, None);
}

#[test]
fn imported_network_preserves_the_service_view() {
    // Law: what riders see is preserved through the codec round trip
    // (feed → inferred document → materialized service view), except
    // for the documented drop (single-stop trips).
    let date = chrono::NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap();
    let original: Vec<(u32, String)> = {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&fixture_zip()).unwrap();
        gtfs.next_departures("A", date, 0, 10)
            .iter()
            .map(|d| (d.departure_time.unwrap(), d.route_id.clone()))
            .collect()
    };
    let network = {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&fixture_zip()).unwrap();
        gtfs_to_network(&gtfs).network
    };
    let derived = network_to_gtfs(&network).unwrap();
    let at_a: Vec<(u32, String)> = derived
        .next_departures("A", date, 0, 10)
        .iter()
        .map(|d| (d.departure_time.unwrap(), d.route_id.clone()))
        .collect();
    assert_eq!(at_a, original, "departures at A survive the round trip");
}

#[test]
fn imported_alignment_survives_materialization() {
    // Law: a pattern's authored path is the geometry the derived feed
    // draws — trips reference it as their shape, so rider views after
    // a planner publish follow the real alignment, not stop chords.
    let network = {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&fixture_zip()).unwrap();
        gtfs_to_network(&gtfs).network
    };
    let derived = network_to_gtfs(&network).unwrap();

    let points = derived.shapes.get("pat-10-0-0").expect("pattern path exported");
    assert_eq!(points.len(), 3);
    assert_eq!(points[0].shape_pt_sequence, 1);

    let trip = derived.trips.get("WK:pat-10-0-0@28800").expect("derived trip id scheme");
    assert_eq!(trip.shape_id.as_deref(), Some("pat-10-0-0"));

    // And the geometry riders actually draw is the polyline, not the
    // stop-to-stop fallback.
    let path = derived.trip_path("WK:pat-10-0-0@28800");
    assert_eq!(path.len(), 3, "trip_path follows the shape: {path:?}");
    assert_eq!(path[0], (45.400, -73.400));

    // Unauthored patterns keep the fallback: no shape reference.
    let bare = derived.trips.get("WK:pat-20-0-0@30000").expect("t20 derived trip");
    assert_eq!(bare.shape_id, None);
}
