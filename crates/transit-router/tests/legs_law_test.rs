//! What a transit leg tells the rider (QAF-67/68).
//!
//! The intent, as the user stated it: a transit leg carries the
//! route's long name alongside its short name, and the headsign the
//! bus shows WHERE THE RIDER BOARDS — the one shared rule
//! (`transit_network::visit_destination`): the visit's own headsign,
//! else the trip's, else the final stop named as the place (its
//! parent station), never the bay — and never the authored pattern's
//! name.
//!
//! The fixture:
//!
//! * t1 (route 12 "Fourth Plain", trip headsign "Downtown"):
//!   A → B → C — boarding anywhere reads "Downtown";
//! * t2 (route 12, no trip headsign, stop headsign "Via Main" at A
//!   only): A → B → BAY (a bay of "Van Mall TC") — boarding at A reads
//!   "Via Main", boarding at B reads "Van Mall TC";
//! * t3 (route 7, short name only): C → BAY, no headsigns — "Van
//!   Mall TC"; its long name is absent.

use std::io::Write;

use chrono::NaiveDate;
use transit_router::{earliest_arrival, Leg, RouterNetwork};

#[path = "prop_support.rs"]
mod support;
use support::model_day;

const DATE: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(); // Friday

fn fixture() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };
    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt",
        "route_id,agency_id,route_short_name,route_long_name,route_type\n12,1,12,Fourth Plain,3\n7,1,7,,3\n");
    // Stops ~1 km apart: no walking shortcuts.
    add(&mut w, "stops.txt",
        "stop_id,stop_name,stop_lat,stop_lon,location_type,parent_station\n\
         A,A St,45.50,-122.50,0,\n\
         B,B St,45.51,-122.50,0,\n\
         C,C St,45.52,-122.50,0,\n\
         VM,Van Mall TC,45.53,-122.50,1,\n\
         BAY,Van Mall TC: Bay D,45.53,-122.50,0,VM\n");
    add(&mut w, "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nALL,1,1,1,1,1,1,1,20260101,20261231\n");
    add(&mut w, "trips.txt",
        "route_id,service_id,trip_id,trip_headsign\n12,ALL,t1,Downtown\n12,ALL,t2,\n7,ALL,t3,\n");
    add(&mut w, "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence,stop_headsign\n\
         t1,08:00:00,08:00:00,A,1,\n\
         t1,08:10:00,08:10:00,B,2,\n\
         t1,08:20:00,08:20:00,C,3,\n\
         t2,09:00:00,09:00:00,A,1,Via Main\n\
         t2,09:10:00,09:10:00,B,2,\n\
         t2,09:20:00,09:20:00,BAY,3,\n\
         t3,10:00:00,10:00:00,C,1,\n\
         t3,10:10:00,10:10:00,BAY,2,\n");
    w.finish().unwrap().into_inner()
}

/// The one transit leg of the plan from `from` leaving at `depart`.
fn ride(net: &RouterNetwork, from: &str, to: &str, depart: u32) -> Leg {
    let it = earliest_arrival(net, from, to, depart, 1).unwrap().expect("reachable");
    let [leg] = &it.legs[..] else { panic!("one ride, got {:?}", it.legs) };
    leg.clone()
}

fn signs(leg: &Leg) -> (String, Option<String>, String) {
    match leg {
        Leg::Transit { route_name, route_long_name, headsign, .. } => {
            (route_name.clone(), route_long_name.clone(), headsign.clone())
        }
        other => panic!("a transit leg, got {other:?}"),
    }
}

#[test]
fn a_leg_names_its_route_and_the_sign_where_the_rider_boards() {
    let (network, day) = model_day(&fixture(), DATE);
    let net = RouterNetwork::build_from_day(&network, &day, DATE);
    let long = Some("Fourth Plain".to_string());

    // t1: the trip's headsign, wherever you board.
    assert_eq!(signs(&ride(&net, "A", "C", 7 * 3600)), ("12".into(), long.clone(), "Downtown".into()));
    assert_eq!(signs(&ride(&net, "B", "C", 7 * 3600)), ("12".into(), long.clone(), "Downtown".into()));
    // t2: the visit's own headsign at A; at B, no visit or trip
    // headsign — the final stop as the place (the station, not the bay).
    assert_eq!(signs(&ride(&net, "A", "BAY", 8 * 3600 + 30 * 60)), ("12".into(), long.clone(), "Via Main".into()));
    assert_eq!(signs(&ride(&net, "B", "BAY", 8 * 3600 + 30 * 60)), ("12".into(), long, "Van Mall TC".into()));
    // t3: a route with no long name says so.
    assert_eq!(signs(&ride(&net, "C", "BAY", 9 * 3600 + 30 * 60)), ("7".into(), None, "Van Mall TC".into()));
}
