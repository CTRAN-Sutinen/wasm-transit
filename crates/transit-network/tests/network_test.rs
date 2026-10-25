//! Example-based tests for the authored network model: a hand-built
//! fixture with hand-computed expectations, serde shape contracts,
//! calendar rules, and one rejection test per validation invariant.
//! The property tests in `prop_test.rs` generalize all of this.

use chrono::NaiveDate;
use transit_network::{
    validation_issues, Anchor, Block, CalendarException, CalendarRule, DateRange, DayState,
    Layover, Mode, Network, NetworkError, Pattern, PatternSchedule, PatternStop, Route, Schedule, ServiceType,
    HeadwayBand, TimingProfile, TripRef, WeekdaySet,
};

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn stop(id: &str, lat: f64, lon: f64) -> (String, transit_network::Stop) {
    use transit_network::Stop;
    (
        id.into(),
        Stop {
            name: format!("Stop {id}"),
            lat,
            lon,
            parent: None,
            transfer: false,
        },
    )
}

/// A small valid network: A → B → C, one inbound pattern, weekday
/// service, one block of two trips with a layover.
pub(crate) fn valid_network() -> Network {
    let net = Network {
        name: "Test Transit".into(),
        anchor: Anchor::NoonMinusTwelve,
        stops: [
            stop("A", 45.0, -122.7),
            stop("B", 45.01, -122.7),
            stop("C", 45.02, -122.7),
        ]
        .into_iter()
        .collect(),
        routes: [(
            "5".into(),
            Route {
                name: "5-K Ave".into(),
                mode: Mode::Bus,
                color: None,
            },
        )]
        .into_iter()
        .collect(),
        patterns: [(
            "5-inbound".into(),
            Pattern {
                route: "5".into(),
                name: "5 inbound".into(),
                path: None,
                stops: vec![
                    PatternStop {
                        stop: "A".into(),
                        timepoint: true,
                        distance_m: 0.0,
                    },
                    PatternStop {
                        stop: "B".into(),
                        timepoint: false,
                        distance_m: 1000.0,
                    },
                    PatternStop {
                        stop: "C".into(),
                        timepoint: true,
                        distance_m: 2000.0,
                    },
                ],
                timing: vec![TimingProfile {
                    start_secs: 0,
                    end_secs: 30 * 3600,
                    at_timepoints: vec![0, 1200],
                }],
            },
        )]
        .into_iter()
        .collect(),
        service_types: [(
            "weekday".into(),
            ServiceType {
                name: "Weekday".into(),
                rule: CalendarRule {
                    days: WeekdaySet::weekdays(),
                    ranges: vec![DateRange {
                        start: date(2026, 1, 1),
                        end: date(2026, 12, 31),
                    }],
                    exceptions: vec![],
                },
            },
        )]
        .into_iter()
        .collect(),
        schedules: [(
            "5-weekday".into(),
            PatternSchedule {
                pattern: "5-inbound".into(),
                service_type: "weekday".into(),
                schedule: Schedule::ExactTimes {
                    starts: vec![7 * 3600, 8 * 3600, 25 * 3600 + 1800],
                },
            },
        )]
        .into_iter()
        .collect(),
        blocks: [(
            "b1".into(),
            Block {
                trips: vec![
                    TripRef {
                        schedule: "5-weekday".into(),
                        start_secs: 7 * 3600,
                    },
                    TripRef {
                        schedule: "5-weekday".into(),
                        start_secs: 8 * 3600,
                    },
                ],
                layovers: vec![Layover {
                    after_trip: 0,
                    secs: 1800,
                    at: Some("C".into()),
                }],
            },
        )]
        .into_iter()
        .collect(),
    };
    net.validate().expect("fixture must be valid");
    net
}

#[test]
fn fixture_validates_and_serializes_as_expected() {
    let net = valid_network();
    let json = serde_json::to_string(&net).unwrap();
    // Shape contracts for the document format.
    assert!(json.contains("\"anchor\":\"noon_minus_twelve\""), "{json}");
    assert!(json.contains("\"mode\":\"bus\""), "{json}");
    assert!(json.contains("\"kind\":\"exact_times\""), "{json}");
    assert!(json.contains("\"start\":\"2026-01-01\""), "{json}");
    assert!(!json.contains("null"), "no nulls: {json}");

    let back: Network = serde_json::from_str(&json).unwrap();
    assert_eq!(back, net);
}

#[test]
fn serialization_is_canonical_regardless_of_insertion_order() {
    let a = valid_network();
    // Same content, maps rebuilt in reverse insertion order: BTreeMap
    // keying must produce byte-identical JSON — that is what makes
    // the document mergeable and content-addressable later.
    let mut b = a.clone();
    b.stops = a.stops.iter().rev().map(|(k, v)| (k.clone(), v.clone())).collect();
    b.patterns = a.patterns.iter().rev().map(|(k, v)| (k.clone(), v.clone())).collect();
    b.blocks = a.blocks.iter().rev().map(|(k, v)| (k.clone(), v.clone())).collect();
    assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
}

#[test]
fn materializes_expected_stop_times_including_interpolation() {
    let net = valid_network();
    let friday = date(2026, 10, 23);
    let trips = transit_network::materialize(&net, friday).unwrap();

    assert_eq!(trips.len(), 3, "three exact trips");
    assert_eq!(trips[0].start_secs, 7 * 3600);
    assert_eq!(trips[0].stop_times[0].stop, "A");
    assert_eq!(trips[0].stop_times[0].departure_secs, 7 * 3600);
    // B interpolates halfway (1000m of 2000m) into the 1200s run.
    assert_eq!(trips[0].stop_times[1].arrival_secs, 7 * 3600 + 600);
    assert_eq!(trips[0].stop_times[1].departure_secs, 7 * 3600 + 600);
    assert_eq!(trips[0].stop_times[2].arrival_secs, 7 * 3600 + 1200);
    // After-midnight trip (25:30) flows through untouched.
    assert_eq!(trips[2].start_secs, 25 * 3600 + 1800);
    assert_eq!(trips[2].stop_times[2].arrival_secs, 25 * 3600 + 1800 + 1200);
    // Sorted by start time.
    assert!(trips.windows(2).all(|w| w[0].start_secs <= w[1].start_secs));

    let saturday = date(2026, 10, 24);
    assert!(transit_network::materialize(&net, saturday).unwrap().is_empty());
    let out_of_range = date(2027, 1, 1);
    assert!(transit_network::materialize(&net, out_of_range).unwrap().is_empty());
}

#[test]
fn calendar_rules_weekdays_ranges_and_exceptions() {
    let rule = CalendarRule {
        days: WeekdaySet::weekdays(),
        ranges: vec![DateRange {
            start: date(2026, 10, 1),
            end: date(2026, 10, 31),
        }],
        exceptions: vec![
            CalendarException {
                date: date(2026, 10, 23),
                state: DayState::Removed,
            },
            CalendarException {
                date: date(2026, 10, 25),
                state: DayState::Added,
            },
        ],
    };
    assert!(!rule.runs_on(date(2026, 10, 23)), "exception removes a Friday");
    assert!(rule.runs_on(date(2026, 10, 22)), "regular Thursday");
    assert!(!rule.runs_on(date(2026, 10, 24)), "regular Saturday");
    assert!(rule.runs_on(date(2026, 10, 25)), "exception adds a Sunday");
    assert!(!rule.runs_on(date(2026, 11, 6)), "a Friday outside the range");
}

// ---------------------------------------------------------------------
// Every validation invariant rejects its defect (example-based; the
// property suite generalizes)
// ---------------------------------------------------------------------

fn first_issue(net: &Network) -> NetworkError {
    validation_issues(net).into_iter().next().unwrap()
}

fn defect<F>(mutate: F) -> NetworkError
where
    F: FnOnce(&mut Network),
{
    let mut net = valid_network();
    mutate(&mut net);
    first_issue(&net)
}

#[test]
fn rejects_broken_references() {
    assert!(matches!(
        defect(|n| n.stops.get_mut("A").unwrap().parent = Some("ghost".into())),
        NetworkError::UnknownParentStop { .. }
    ));
    assert!(matches!(
        defect(|n| n.patterns.get_mut("5-inbound").unwrap().stops[1].stop = "ghost".into()),
        NetworkError::UnknownStop { .. }
    ));
    assert!(matches!(
        defect(|n| n.patterns.get_mut("5-inbound").unwrap().route = "ghost".into()),
        NetworkError::UnknownRoute { .. }
    ));
    assert!(matches!(
        defect(|n| n.schedules.get_mut("5-weekday").unwrap().pattern = "ghost".into()),
        NetworkError::UnknownPattern { .. }
    ));
    assert!(matches!(
        defect(|n| n.schedules.get_mut("5-weekday").unwrap().service_type = "ghost".into()),
        NetworkError::UnknownServiceType { .. }
    ));
    assert!(matches!(
        defect(|n| n.blocks.get_mut("b1").unwrap().trips[0].schedule = "ghost".into()),
        NetworkError::UnknownBlockSchedule { .. }
    ));
    assert!(matches!(
        defect(|n| n.blocks.get_mut("b1").unwrap().layovers[0].at = Some("ghost".into())),
        NetworkError::UnknownLayoverStop { .. }
    ));
}

#[test]
fn rejects_broken_pattern_structure() {
    use NetworkError::*;
    assert!(matches!(defect(|n| {
        let only = n.patterns["5-inbound"].stops[0].clone();
        n.patterns.get_mut("5-inbound").unwrap().stops = vec![only];
    }), PatternTooShort { .. }));
    assert!(matches!(
        defect(|n| n.patterns.get_mut("5-inbound").unwrap().stops[0].timepoint = false),
        EndpointsMustBeTimepoints { .. }
    ));
    assert!(matches!(
        defect(|n| n.patterns.get_mut("5-inbound").unwrap().timing = vec![]),
        NoTimingProfile { .. }
    ));
    assert!(matches!(defect(|n| {
        n.patterns.get_mut("5-inbound").unwrap().timing[0].end_secs = 0;
    }), EmptyTimingProfile { .. }));
    assert!(matches!(defect(|n| {
        n.patterns.get_mut("5-inbound").unwrap().timing[0].at_timepoints = vec![0];
    }), TimepointCountMismatch { .. }));
    assert!(matches!(defect(|n| {
        n.patterns.get_mut("5-inbound").unwrap().timing[0].at_timepoints = vec![1, 1200];
    }), FirstTimepointNotZero { .. }));
    assert!(matches!(defect(|n| {
        n.patterns.get_mut("5-inbound").unwrap().timing[0].at_timepoints = vec![0, 900, 1200];
    }), TimepointCountMismatch { .. }));
    // 3 timepoints, times out of order.
    assert!(matches!(defect(|n| {
        let p = n.patterns.get_mut("5-inbound").unwrap();
        p.stops[1].timepoint = true;
        p.timing[0].at_timepoints = vec![0, 1200, 900];
    }), NonMonotonicTimes { .. }));
    assert!(matches!(defect(|n| {
        let p = n.patterns.get_mut("5-inbound").unwrap();
        p.timing.push(TimingProfile { start_secs: 1800, end_secs: 7200, at_timepoints: vec![0, 1200] });
    }), OverlappingTimingProfiles { .. }));
    assert!(matches!(defect(|n| {
        n.patterns.get_mut("5-inbound").unwrap().stops[1].distance_m = 3000.0;
    }), DecreasingDistances { .. }));
}

#[test]
fn rejects_broken_service_structure() {
    use NetworkError::*;
    assert!(matches!(defect(|n| {
        n.schedules.get_mut("5-weekday").unwrap().schedule = Schedule::ExactTimes { starts: vec![] };
    }), EmptySchedule { .. }));
    assert!(matches!(defect(|n| {
        n.schedules.get_mut("5-weekday").unwrap().schedule = Schedule::HeadwayBands {
            bands: vec![HeadwayBand { start_secs: 0, end_secs: 3600, headway_secs: 0 }],
        };
    }), ZeroHeadway { .. }));
    assert!(matches!(defect(|n| {
        n.schedules.get_mut("5-weekday").unwrap().schedule = Schedule::HeadwayBands {
            bands: vec![HeadwayBand { start_secs: 3600, end_secs: 3600, headway_secs: 300 }],
        };
    }), EmptyHeadwayBand { .. }));
    assert!(matches!(defect(|n| {
        n.schedules.get_mut("5-weekday").unwrap().schedule = Schedule::HeadwayBands {
            bands: vec![
                HeadwayBand { start_secs: 0, end_secs: 3600, headway_secs: 300 },
                HeadwayBand { start_secs: 1800, end_secs: 7200, headway_secs: 300 },
            ],
        };
    }), OverlappingHeadwayBands { .. }));
    assert!(matches!(defect(|n| {
        n.schedules.get_mut("5-weekday").unwrap().schedule =
            Schedule::ExactTimes { starts: vec![8 * 3600, 7 * 3600] };
    }), UnsortedExactTimes { .. }));
    // Trip outside every timing profile.
    assert!(matches!(defect(|n| {
        n.patterns.get_mut("5-inbound").unwrap().timing[0].end_secs = 6 * 3600;
    }), TripOutsideTimingProfiles { .. }));
    assert!(matches!(defect(|n| {
        n.service_types.get_mut("weekday").unwrap().rule.ranges[0].start = date(2027, 1, 1);
    }), BadDateRange { .. }));
}

#[test]
fn rejects_broken_runcut() {
    use NetworkError::*;
    assert!(matches!(defect(|n| {
        n.blocks.get_mut("b1").unwrap().trips = vec![];
    }), EmptyBlock { .. }));
    assert!(matches!(defect(|n| {
        let b = n.blocks.get_mut("b1").unwrap();
        b.trips.swap(0, 1);
    }), BlockOutOfOrder { .. }));
    assert!(matches!(defect(|n| {
        n.blocks.get_mut("b1").unwrap().layovers[0].after_trip = 1;
    }), LayoverAfterLastTrip { .. }));
    // Second trip departs before the first arrives (arrival 7:20).
    assert!(matches!(defect(|n| {
        n.blocks.get_mut("b1").unwrap().trips[1].start_secs = 7 * 3600 + 60;
    }), BlockInfeasible { .. }));
    // Arrives in time, but the layover does not fit before 8:00.
    assert!(matches!(defect(|n| {
        n.blocks.get_mut("b1").unwrap().layovers[0].secs = 3 * 3600;
    }), LayoverInfeasible { .. }));
    // The same trip in two blocks.
    assert!(matches!(defect(|n| {
        n.blocks.insert("b2".into(), Block {
            trips: vec![TripRef { schedule: "5-weekday".into(), start_secs: 8 * 3600 }],
            layovers: vec![],
        });
    }), DuplicateBlockTrip { .. }));
}

#[test]
fn materialization_is_total_on_invalid_networks() {
    // No panics on documents that failed validation — the wasm seam
    // must never trap on hostile input.
    let mut net = valid_network();
    net.patterns.get_mut("5-inbound").unwrap().timing = vec![];
    assert!(transit_network::materialize(&net, date(2026, 10, 23)).is_err());

    let mut net = valid_network();
    net.schedules.get_mut("5-weekday").unwrap().service_type = "ghost".into();
    assert!(matches!(
        transit_network::materialize(&net, date(2026, 10, 23)),
        Err(NetworkError::UnknownServiceType { .. })
    ));

    let mut net = valid_network();
    net.blocks.insert("b".into(), Block {
        trips: vec![TripRef { schedule: "ghost".into(), start_secs: 0 }],
        layovers: vec![],
    });
    assert!(transit_network::validation_issues(&net)
        .iter()
        .any(|e| matches!(e, NetworkError::UnknownBlockSchedule { .. })));
}

#[test]
fn transfer_point_requires_two_routes_across_its_walking_group() {
    // The flag is composed intent: flagged on a stop whose group is
    // served by one route only, it is meaningless.
    let mut net = valid_network();
    net.stops.get_mut("B").unwrap().transfer = true;
    assert!(matches!(
        net.validate(),
        Err(NetworkError::TransferPointWithoutConnections { stop }) if stop == "B"
    ));

    // A second route meeting across the street — a sibling stop
    // sharing B's parent station — makes the flag compose: the
    // walking group, not the pin, is the unit.
    let mut net = valid_network();
    net.stops.insert("STN".into(), transit_network::Stop {
        name: "Transfer Center".into(),
        lat: 45.01, lon: -122.701,
        parent: None,
        transfer: false,
    });
    let b2 = transit_network::Stop {
        name: "Stop B across the street".into(),
        lat: 45.0101, lon: -122.701,
        parent: Some("STN".into()),
        transfer: false,
    };
    net.stops.insert("B2".into(), b2.clone());
    let mut b = net.stops.get_mut("B").unwrap();
    b.parent = Some("STN".into());
    b.transfer = true;
    net.routes.insert("7".into(), Route {
        name: "7-L St".into(),
        mode: Mode::Bus,
        color: None,
    });
    net.patterns.insert("7-inbound".into(), Pattern {
        route: "7".into(),
        name: "7 inbound".into(),
        path: None,
        stops: vec![
            PatternStop { stop: "B2".into(), timepoint: true, distance_m: 0.0 },
            PatternStop { stop: "A".into(), timepoint: true, distance_m: 900.0 },
        ],
        timing: vec![TimingProfile {
            start_secs: 0,
            end_secs: 30 * 3600,
            at_timepoints: vec![0, 600],
        }],
    });
    assert!(net.validate().is_ok(), "two routes across the walking group compose: {:?}", validation_issues(&net));

    // The helpers say the same thing: the group is B, B2, STN; the
    // routes meeting there are 5 and 7.
    let group = transit_network::walking_group(&net, "B");
    assert_eq!(group.len(), 3, "walking group: {:?}", group.keys().collect::<Vec<_>>());
    let index = transit_network::stop_route_index(&net);
    let meeting = transit_network::routes_meeting_at(&net, &index, "B");
    assert!(meeting.contains("5") && meeting.contains("7"), "{meeting:?}");
}
