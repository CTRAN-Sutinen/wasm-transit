//! A shuttle world for the runs laws: stops A and C (and C2, a bay of
//! station CS with C), routes 5 and 6 running A → C and back, every
//! trip 20 minutes, weekday service. Trips are built from
//! (route, outbound, start) triples; blocks are optional.

#![allow(dead_code)]

use chrono::NaiveDate;
use transit_network::{
    Anchor, Block, CalendarRule, DateRange, Departs, Layover, Mode, Network, Pattern, PatternStop, PatternTrips, Route,
    ServiceType, Stop, StopOffset, Trip, TripRef, WeekdaySet,
};

pub const H: u32 = 3600;
pub const RUN: u32 = 1200;

/// A weekday (Friday).
pub fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()
}

fn pattern(route: &str, from: &str, to: &str) -> Pattern {
    Pattern {
        route: route.into(),
        name: format!("{route} {from}→{to}"),
        stops: vec![
            PatternStop { stop: from.into(), timepoint: true, distance_m: 0.0, ..Default::default() },
            PatternStop { stop: to.into(), timepoint: true, distance_m: 2000.0, ..Default::default() },
        ],
        ..Default::default()
    }
}

/// The trip id of a triple.
pub fn trip_id(route: &str, outbound: bool, start: u32) -> String {
    format!("{route}{}@{start}", if outbound { "o" } else { "i" })
}

/// The world with these trips and blocks (each block: trip ids in
/// order, layovers left to the caller).
pub fn world(trips: &[(&str, bool, u32)], blocks: &[(&str, Vec<String>)]) -> Network {
    let stop = |lat: f64, parent: Option<&str>| Stop {
        name: "s".into(),
        lat,
        lon: -122.7,
        parent: parent.map(String::from),
        ..Default::default()
    };
    let mut net = Network {
        name: "Shuttle".into(),
        anchor: Anchor::NoonMinusTwelve,
        stops: [
            ("A".to_string(), stop(45.0, None)),
            ("C".to_string(), stop(45.02, Some("CS"))),
            ("C2".to_string(), stop(45.0201, Some("CS"))),
            ("CS".to_string(), stop(45.02, None)),
        ]
        .into_iter()
        .collect(),
        routes: ["5", "6"]
            .into_iter()
            .map(|r| (r.to_string(), Route { name: r.into(), mode: Mode::Bus, ..Default::default() }))
            .collect(),
        patterns: [
            ("5o".to_string(), pattern("5", "A", "C")),
            ("5i".to_string(), pattern("5", "C", "A")),
            ("6o".to_string(), pattern("6", "A", "C2")),
            ("6i".to_string(), pattern("6", "C2", "A")),
        ]
        .into_iter()
        .collect(),
        service_types: [(
            "wk".to_string(),
            ServiceType {
                name: "Weekday".into(),
                rule: CalendarRule {
                    days: WeekdaySet::weekdays(),
                    ranges: vec![DateRange {
                        start: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                        end: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
                    }],
                    exceptions: vec![],
                },
            },
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    for &(route, outbound, start) in trips {
        let pattern = format!("{route}{}", if outbound { "o" } else { "i" });
        let at = |t: u32| StopOffset { arrival_secs: Some(t), departure_secs: Some(t) };
        net.pattern_trips
            .entry(format!("{pattern}-wk"))
            .or_insert_with(|| PatternTrips { pattern: pattern.clone(), service_type: "wk".into(), trips: vec![] })
            .trips
            .push(Trip {
                id: trip_id(route, outbound, start),
                departs: Departs::At { start_secs: start },
                times: vec![at(0), at(RUN)],
                ..Default::default()
            });
    }
    for pt in net.pattern_trips.values_mut() {
        pt.trips.sort_by_key(|t| t.departs.first_start());
        pt.trips.dedup_by(|a, b| a.id == b.id);
    }
    for (id, members) in blocks {
        net.blocks.insert(
            id.to_string(),
            Block { trips: members.iter().map(|t| TripRef { trip: t.clone() }).collect(), layovers: vec![] },
        );
    }
    net
}

/// A layover after trip `after` of block `block`.
pub fn layover(net: &mut Network, block: &str, after: usize, secs: u32) {
    net.blocks.get_mut(block).unwrap().layovers.push(Layover { after_trip: after, secs, at: None });
}
