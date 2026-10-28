//! Scenario tests on a hand-built feed:
//!
//! A --t1--> B --t2--> C      (t0 runs A -> C directly, but later)
//!                 C --walk--> D
//!
//! Planning A -> C must transfer at B (08:30 beats t0's 09:10);
//! planning A -> D must additionally walk the last leg; D -> A is
//! unreachable.

use std::io::Write;

use chrono::NaiveDate;
use transit_router::{earliest_arrival, plan_notice, Leg, RouterNetwork};

#[path = "prop_support.rs"]
mod support;
use support::model_day;

const DATE: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(); // Friday

fn router(zip: &[u8]) -> RouterNetwork {
    let (network, day) = model_day(zip, DATE);
    RouterNetwork::build_from_day(&network, &day, DATE)
}

fn fixture_zip() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };

    add(&mut w, "agency.txt",
        "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt",
        "route_id,agency_id,route_short_name,route_type\n1,1,1,3\n2,1,2,3\n");
    add(&mut w, "stops.txt",
        "stop_id,stop_name,stop_lat,stop_lon\nA,A Stop,45.500,-122.500\nB,B Stop,45.510,-122.510\nC,C Stop,45.520,-122.520\nD,D Stop,45.5205,-122.520\n");
    add(&mut w, "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nALL,1,1,1,1,1,1,1,20260101,20261231\n");
    add(&mut w, "trips.txt",
        "route_id,service_id,trip_id\n1,ALL,t0\n1,ALL,t1\n2,ALL,t2\n");
    add(&mut w, "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt1,08:00:00,08:00:00,A,1\nt1,08:10:00,08:10:00,B,2\nt2,08:20:00,08:20:00,B,1\nt2,08:30:00,08:30:00,C,2\nt0,09:00:00,09:00:00,A,1\nt0,09:10:00,09:10:00,C,2\n");

    w.finish().unwrap().into_inner()
}

#[test]
fn transfers_beat_later_direct_trips() {
    let net = router(&fixture_zip());
    let it = earliest_arrival(&net, "A", "C", 7 * 3600 + 30 * 60, 3)
        .expect("query")
        .expect("A -> C reachable");

    assert_eq!(it.arrival_hhmmss, "08:30:00"); // t1 + t2, not t0's 09:10
    assert_eq!(it.legs.len(), 2, "two transit legs");
    match &it.legs[0] {
        Leg::Transit { trip, start_secs, board_stop_id, alight_stop_id, .. } => {
            // The trip instance ridden: the network's trip (the feed's
            // own id) and its start — t1 departs A at 08:00.
            assert_eq!((trip.as_str(), *start_secs), ("t1", 8 * 3600));
            assert_eq!(board_stop_id, "A");
            assert_eq!(alight_stop_id, "B");
        }
        other => panic!("expected transit leg, got {other:?}"),
    }
    match &it.legs[1] {
        Leg::Transit { start_secs, board_stop_id, alight_stop_id, .. } => {
            assert_eq!(*start_secs, 8 * 3600 + 20 * 60);
            assert_eq!(board_stop_id, "B");
            assert_eq!(alight_stop_id, "C");
        }
        other => panic!("expected transit leg, got {other:?}"),
    }
}

#[test]
fn walking_extends_the_plan() {
    let net = router(&fixture_zip());
    let it = earliest_arrival(&net, "A", "D", 7 * 3600 + 30 * 60, 3)
        .expect("query")
        .expect("A -> D reachable via walk from C");

    // t1 + t2 + the C -> D footpath.
    assert_eq!(it.legs.len(), 3);
    let walk_secs = match &it.legs[2] {
        Leg::Walk { from_stop_id, to_stop_id, walk_secs, .. } => {
            assert_eq!(from_stop_id, "C");
            assert_eq!(to_stop_id, "D");
            // ~55 m at 1.3 m/s.
            assert!((30..90).contains(walk_secs), "implausible walk_secs {walk_secs}");
            *walk_secs
        }
        other => panic!("expected walk leg, got {other:?}"),
    };
    assert_eq!(it.arrival_secs, 8 * 3600 + 30 * 60 + walk_secs);
}

#[test]
fn unreachable_and_trivial_queries() {
    let net = router(&fixture_zip());

    // D has no transit and nothing walks anywhere useful from it.
    let plan = earliest_arrival(&net, "D", "A", 8 * 3600, 4).expect("query");
    assert!(plan.is_none(), "D -> A should be unreachable");

    // Same stop: zero legs, arrival = departure.
    let it = earliest_arrival(&net, "A", "A", 8 * 3600, 4)
        .expect("query")
        .expect("trivially reachable");
    assert!(it.legs.is_empty());
    assert_eq!(it.arrival_secs, 8 * 3600);

    // Unknown stops are errors, not empty plans.
    assert!(earliest_arrival(&net, "ZZZ", "A", 8 * 3600, 4).is_err());
}

#[test]
fn a_compared_plan_tells_the_rider_what_it_cost() {
    let net = router(&fixture_zip());
    let found = earliest_arrival(&net, "A", "B", 8 * 3600, 4).expect("query").expect("A -> B runs");
    assert_eq!(plan_notice(Some(&found), None, 6), None);
    assert_eq!(plan_notice(Some(&found), Some(&found), 6), None);
    let lost = plan_notice(None, Some(&found), 6).unwrap();
    assert!(lost.ends_with(&format!("arrive {}.", &found.arrival_hhmmss[..5])), "{lost}");
    assert_eq!(
        plan_notice(None, None, 6).as_deref(),
        Some("No trip found within 6 legs — try a different time or closer stops.")
    );
}

#[test]
fn calendar_filters_trips_from_the_network() {
    // A second feed where the only A -> B trip runs on Sunday.
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };
    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n1,1,1,3\n");
    add(&mut w, "stops.txt", "stop_id,stop_name,stop_lat,stop_lon\nA,A,45.500,-122.500\nB,B,45.800,-122.500\n"); // far apart: no footpath
    add(&mut w, "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nSUN,0,0,0,0,0,0,1,20260101,20261231\n");
    add(&mut w, "trips.txt", "route_id,service_id,trip_id\n1,SUN,t1\n");
    add(&mut w, "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt1,08:00:00,08:00:00,A,1\nt1,08:10:00,08:10:00,B,2\n");
    let bytes = w.finish().unwrap().into_inner();

    // Calendar filtering is the model's materialize (the day carries
    // only the trips whose service runs); the router rides the day
    // it is handed.
    let friday = {
        let (network, day) = model_day(&bytes, NaiveDate::from_ymd_opt(2026, 10, 23).unwrap());
        RouterNetwork::build_from_day(&network, &day, NaiveDate::from_ymd_opt(2026, 10, 23).unwrap())
    };
    assert!(earliest_arrival(&friday, "A", "B", 0, 4).expect("q").is_none(),
        "trip does not run on Friday");

    let sunday = {
        let (network, day) = model_day(&bytes, NaiveDate::from_ymd_opt(2026, 10, 25).unwrap());
        RouterNetwork::build_from_day(&network, &day, NaiveDate::from_ymd_opt(2026, 10, 25).unwrap())
    };
    let it = earliest_arrival(&sunday, "A", "B", 0, 4).expect("q").expect("runs Sunday");
    assert_eq!(it.arrival_hhmmss, "08:10:00");
}
