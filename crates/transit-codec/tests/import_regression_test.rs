//! Import regression tests: the shapes found importing REAL agency
//! feeds (C-TRAN) that the import must handle — production trip ids
//! that do not sort by start time, duplicated starts, per-trip
//! run-time variation inside one pattern, impossible block chains,
//! aspirational transfers — plus the rider-view and alignment checks
//! of the round trip. Every test builds its feed inline; import is
//! the codec under test, so these live at the codec's own seam.

use std::io::Write;

use transit_codec::{export, Boundary, Feed, ImportOutcome};
use transit_network::Departs;

fn import(bytes: &[u8]) -> ImportOutcome {
    transit_codec::import(&Feed::from_zip_bytes(bytes).expect("the feed reads"))
}

/// Export and read back, as a feed.
fn published(net: &transit_network::Network) -> Feed {
    Feed::from_zip_bytes(&export(net, &Boundary::default()).expect("exports").to_zip_bytes()).expect("reads")
}

/// The pattern trips' trip ids, in stored order.
fn ids(net: &transit_network::Network, key: &str) -> Vec<String> {
    net.pattern_trips[key].trips.iter().map(|t| t.id.clone()).collect()
}

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
    import(&production_zip()).network
}

#[test]
fn import_infers_a_valid_document_from_a_feed() {
    let network = import(&fixture_zip()).network;
    // The inference must produce a document the model accepts — that
    // is the contract that lets the planner edit it immediately.
    assert!(network.validate().is_ok(),
        "inferred document must validate: {:?}", transit_network::validation_issues(&network));

    // Fixture shape (see fixture_zip): 3 stops, 2 routes, four
    // patterns — route 10's A·B with sh1 (t10) and without (t11),
    // its A·C·B (t30), route 20's A·B (t20); t12 is a single-stop
    // trip, dropped and reported — every calendar the feed names (HOL
    // served only t12), four pattern trips.
    assert_eq!(network.stops.len(), 3);
    assert_eq!(network.routes.len(), 2);
    assert_eq!(network.patterns.len(), 4);
    assert_eq!(network.service_types.keys().collect::<Vec<_>>(), ["HOL", "SUN", "WK"]);
    assert_eq!(network.pattern_trips.len(), 4);

    // Route 10's shaped A→B pattern: t10 at 08:00 under WK — one
    // pattern trips per (service, pattern).
    let svc = network.pattern_trips.get("WK:pat-10-0-1").expect("service id scheme");
    assert_eq!(ids(&network, "WK:pat-10-0-1"), ["t10"]);
    assert_eq!(ids(&network, "SUN:pat-10-0-0"), ["t11"]);
    assert_eq!(svc.trips[0].departs, Departs::At { start_secs: 8 * 3600 });
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
    assert_eq!(unshaped.len(), 3, "only pat-10-0-1 has a path: {unshaped:?}");

    // The runcut survives: B1 chains t10 and t20 chronologically.
    let block = network.blocks.get("B1").expect("block B1 kept");
    assert_eq!(block.trips.iter().map(|r| r.trip.as_str()).collect::<Vec<_>>(), ["t10", "t20"]);
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
    // 9 stop_times — and the old parser reads it the same.
    let net: transit_network::Network =
        serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap();
    let feed = published(&net);
    assert_eq!((feed.stops.len(), feed.routes.len(), feed.trips.len(), feed.stop_times.len()), (3, 1, 3, 9));
    let bytes = export(&net, &Boundary::default()).unwrap().to_zip_bytes();
    let summary = transit_core::Gtfs::from_zip_bytes(&bytes).unwrap().summary();
    assert_eq!((summary.stops, summary.routes, summary.trips, summary.stop_times), (3, 1, 3, 9));
}

#[test]
fn duplicate_starts_are_two_trips_in_start_order() {
    // Regression (C-TRAN): trip ids do not sort by start, and real
    // feeds run two vehicles at one start. t1/t3 both leave at 08:00
    // and stay two trips; t9's 06:00 comes first. The four A·B·C
    // trips share one pattern whatever their run times (t9 runs 30
    // minutes, the rest 20); t7's A·B is its own.
    let network = import_production();
    assert_eq!(ids(&network, "WK:pat-10-0-1"), ["t9", "t1", "t3", "t5"]);
    assert_eq!(ids(&network, "WK:pat-10-0-0"), ["t7"]);
}

#[test]
fn import_preserves_each_trips_own_run_time() {
    // Regression (C-TRAN): the timing profile used to be collapsed
    // to one runtime per stop sequence — the shortest member's —
    // which planned boardings that did not exist in the feed (the
    // real 105 trips ran 9–11 minutes past the modeled times). The
    // run times are part of the pattern identity now: every trip
    // materializes its own feed times, and connections that hold
    // in the source feed hold in the model exactly.
    let network = import_production();
    assert!(network.validate().is_ok(),
        "inferred document must validate: {:?}", transit_network::validation_issues(&network));
    // t9's 30-minute run is its own, not a 20-minute trip (the old
    // loss) and not the pattern profile.
    let end = |id: &str| {
        let (_, _, t) = network.trip(id).expect("kept");
        transit_network::trip_arrival_end_secs(t, t.departs.first_start().unwrap()).unwrap()
    };
    assert_eq!(end("t9"), 6 * 3600 + 30 * 60, "t9 keeps its own 30-minute runtime");
    assert_eq!(end("t1"), 8 * 3600 + 20 * 60, "t1 keeps its own 20-minute runtime");
    // The block chain is feasible against every trip's own times
    // (t1 arrives 08:20, t5 departs 08:22).
    let block = network.blocks.get("B1").expect("block B1 kept");
    assert_eq!(block.trips.iter().map(|r| r.trip.as_str()).collect::<Vec<_>>(), ["t9", "t1", "t5"]);
    // The whole point: the document publishes.
    assert!(export(&network, &Boundary::default()).is_ok());
}

/// A feed outside the spec — an infeasible block chain (departs
/// before the vehicle arrives), two trips of one block at the same
/// second — imports as it is: nothing is dropped from a block, and
/// validation names what one vehicle cannot run. A single-stop trip
/// the model cannot hold is dropped and reported.
#[test]
fn spec_invalid_blocks_import_as_they_are() {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };
    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n");
    add(&mut w, "stops.txt", "stop_id,stop_name,stop_lat,stop_lon\nA,Main & 1st,45.500,-73.500\nB,Main & 2nd,45.510,-73.510\n");
    // t1/t2: block B1, t2 departs 08:05 while t1 arrives 08:30 —
    // genuinely infeasible source data. t3/t4: block B2, both at
    // 09:00. t5: block B3, alone. t6: a single-stop trip.
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

    let ImportOutcome { network, notes, .. } = import(&bytes);
    let trips = |b: &str| network.blocks[b].trips.iter().map(|r| r.trip.clone()).collect::<Vec<_>>();
    assert_eq!(trips("B1"), ["t1", "t2"]);
    assert_eq!(trips("B2"), ["t3", "t4"]);
    assert_eq!(trips("B3"), ["t5"]);
    let blocks: Vec<String> = transit_network::validation_issues(&network)
        .iter()
        .filter_map(|i| match i {
            transit_network::NetworkError::BlockInfeasible { block, .. }
            | transit_network::NetworkError::BlockOutOfOrder { block, .. } => Some(block.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(blocks, ["B1", "B2"], "validation names both vehicles");
    assert_eq!(
        notes,
        [transit_codec::ImportNote::DroppedTrips { single_stop: 1, no_departure: 0, missing_stop_coords: 0 }]
    );
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
    let ImportOutcome { network, notes, .. } = import(&transfer_zip());
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
    // What riders see at A on the Friday, read off the fixture by
    // hand: t10 08:00 (10), t20 08:20 (20), t30 08:40 (10) — WK runs,
    // SUN does not, HOL's only trip starts at B. The import's day and
    // the published feed's re-import answer the same.
    let date = chrono::NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap();
    let at_a = |net: &transit_network::Network| -> Vec<(u32, String)> {
        let mut out: Vec<(u32, String)> = transit_network::materialize(net, date)
            .unwrap()
            .iter()
            .flat_map(|t| {
                t.stop_times[..t.stop_times.len() - 1]
                    .iter()
                    .filter(|st| st.stop == "A")
                    .map(|st| (st.departure_secs, t.route.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        out.sort();
        out
    };
    let expected = vec![(28_800, "10".to_string()), (30_000, "20".to_string()), (31_200, "10".to_string())];
    let network = import(&fixture_zip()).network;
    assert_eq!(at_a(&network), expected);
    let again = transit_codec::import(&published(&network)).network;
    assert_eq!(at_a(&again), expected, "departures at A survive the round trip");
}

#[test]
fn imported_alignment_survives_publishing() {
    // A pattern's authored path is the geometry the published feed
    // draws — its trips reference it as their shape, so rider views
    // after a planner publish follow the real alignment, not stop
    // chords. Unauthored patterns publish no shape.
    let network = import(&fixture_zip()).network;
    let feed = published(&network);
    let points: Vec<&transit_codec::feed::ShapeRow> =
        feed.shapes.iter().filter(|r| r.shape_id.as_deref() == Some("pat-10-0-1")).collect();
    assert_eq!(points.len(), 3);
    assert_eq!(points[0].shape_pt_sequence.as_deref(), Some("1"));
    assert_eq!((points[0].shape_pt_lat.as_deref(), points[0].shape_pt_lon.as_deref()), (Some("45.4"), Some("-73.4")));
    let shape_of = |id: &str| feed.trips.iter().find(|t| t.trip_id.as_deref() == Some(id)).unwrap().shape_id.clone();
    assert_eq!(shape_of("t10").as_deref(), Some("pat-10-0-1"), "the agency's own trip id");
    assert_eq!(shape_of("t20"), None);
}

/// GTFS direction_id survives the import as the route's directions,
/// each named by its trips' most common headsign (else "To" its most
/// common terminal); a trip without one places its pattern in none.
/// The export signs directed trips with the direction's name.
#[test]
fn directions_are_named_from_headsigns_and_survive_the_round_trip() {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };
    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n");
    add(&mut w, "stops.txt", "stop_id,stop_name,stop_lat,stop_lon\nA,Mall,45.500,-73.500\nB,Downtown,45.510,-73.510\nC,Bay C,45.511,-73.511\n");
    add(
        &mut w,
        "trips.txt",
        "route_id,service_id,trip_id,direction_id,trip_headsign\n10,WK,t1,0,Downtown\n10,WK,t2,0,Downtown\n10,WK,t3,0,Downtown via C\n10,WK,t4,1,\n10,WK,t5,,\n",
    );
    add(
        &mut w,
        "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt1,08:00:00,08:00:00,A,1\nt1,08:10:00,08:10:00,B,2\nt2,09:00:00,09:00:00,A,1\nt2,09:10:00,09:10:00,B,2\nt3,10:00:00,10:00:00,A,1\nt3,10:12:00,10:12:00,C,2\nt4,08:30:00,08:30:00,B,1\nt4,08:40:00,08:40:00,A,2\nt5,11:00:00,11:00:00,A,1\nt5,11:10:00,11:10:00,B,2\n",
    );
    add(&mut w, "calendar.txt", "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWK,1,1,1,1,1,0,0,20260101,20261231\n");
    let bytes = w.finish().unwrap().into_inner();
    let net = import(&bytes).network;

    let dirs = &net.routes["10"].directions;
    assert_eq!(dirs.keys().collect::<Vec<_>>(), ["0", "1"]);
    assert_eq!(dirs["0"].name, "Downtown", "the most common headsign; the bay variant shares the direction");
    assert_eq!(dirs["1"].name, "To Mall", "no headsign: the terminal names it");
    let in_dir = |d: Option<&str>| net.patterns.values().filter(|p| p.direction.as_deref() == d).count();
    assert_eq!((in_dir(Some("0")), in_dir(Some("1")), in_dir(None)), (2, 1, 1));
    assert!(transit_network::validation_issues(&net).is_empty());

    let again = transit_codec::import(&published(&net)).network;
    assert_eq!(again.routes, net.routes, "directions and their names survive");
    assert_eq!(again.patterns, net.patterns);
}
