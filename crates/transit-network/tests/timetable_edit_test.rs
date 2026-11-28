//! Timetable gestures (cap-author/OPS.md) over the committed planner network
//! (testdata/network.json).
//!
//! 1. A trip that stays is retimed under its id, so its blocks keep it.
//! 2. A trip that goes leaves its blocks, and the layovers around it go.
//! 3. New trips land in a neighbor's pattern trips, unassigned.
//! 4. Running time changes reach only the trips in the window on the named
//!    service type.
//! 5. A stop change applies to every pattern of the direction, by stop id.
//!    Endpoints stay.
//! 6. What a gesture cannot do is a named error.

use transit_network::{
    apply_edits, materialize, validation_issues, Departs, HeadwayBand, Network, NetworkEdit,
    NetworkWarning, TripRef,
};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap()
}

fn apply(net: &Network, edits: Vec<NetworkEdit>) -> Network {
    apply_edits(net, &edits).expect("the edits apply")
}

fn refused(net: &Network, edit: NetworkEdit) -> String {
    apply_edits(net, &[edit])
        .expect_err("the edit is refused")
        .reason
}

fn starts(net: &Network, id: &str) -> Vec<u32> {
    let mut starts: Vec<u32> = net.pattern_trips[id]
        .trips
        .iter()
        .filter_map(|t| t.departs.first_start())
        .collect();
    starts.sort_unstable();
    starts
}

fn block_starts(net: &Network) -> Vec<u32> {
    net.blocks["400"]
        .trips
        .iter()
        .map(|t| net.trip(&t.trip).unwrap().2.departs.first_start().unwrap())
        .collect()
}

fn headway(from: u32, to: u32, every: u32) -> NetworkEdit {
    NetworkEdit::SetHeadway {
        route: "10".into(),
        direction: None,
        service_type: "weekday".into(),
        from_secs: from,
        to_secs: to,
        headway_secs: every,
        pattern: None,
    }
}

const H: u32 = 3600;

#[test]
fn a_higher_frequency_keeps_the_trips_that_run_and_adds_the_rest() {
    let net = network();
    let next = apply(&net, vec![headway(8 * H, 9 * H, 900)]);
    assert_eq!(
        starts(&next, "10-weekday"),
        [28_800, 29_700, 30_600, 31_500, 91_800]
    );
    assert_eq!(
        block_starts(&next),
        [28_800, 30_600],
        "the runcut is untouched"
    );
    assert_eq!(next.blocks["400"].layovers, net.blocks["400"].layovers);
    assert!(validation_issues(&next).is_empty());
    let unassigned = transit_network::validation_warnings(&next);
    assert!(
        unassigned
            .iter()
            .any(|w| matches!(w, NetworkWarning::UnassignedTrips { trips: 3, .. })),
        "{unassigned:?}"
    );
}

#[test]
fn a_new_clockface_moves_trips_with_their_blocks() {
    let next = apply(&network(), vec![headway(7 * H + 3000, 9 * H, 1800)]);
    assert_eq!(
        starts(&next, "10-weekday"),
        [28_200, 30_000, 31_800, 91_800]
    );
    assert_eq!(
        block_starts(&next),
        [28_200, 30_000],
        "the block keeps the moved trips"
    );
    assert_eq!(next.blocks, network().blocks, "by id");
    assert_eq!(
        next.blocks["400"].layovers.len(),
        1,
        "the layover between them stays"
    );
    assert!(
        validation_issues(&next).is_empty(),
        "{:?}",
        validation_issues(&next)
    );
}

#[test]
fn a_lower_frequency_drops_trips_and_closes_the_chain() {
    let next = apply(&network(), vec![headway(8 * H, 9 * H, H)]);
    assert_eq!(starts(&next, "10-weekday"), [28_800, 91_800]);
    assert_eq!(block_starts(&next), [28_800]);
    assert!(
        next.blocks["400"].layovers.is_empty(),
        "the layover before the dropped trip goes with it"
    );
    assert!(validation_issues(&next).is_empty());
}

#[test]
fn the_span_trims_and_extends_at_the_edge_headway() {
    let net = network();
    let span = |first: Option<u32>, last: Option<u32>| NetworkEdit::SetSpan {
        route: "10".into(),
        direction: None,
        service_type: "weekday".into(),
        first_secs: first,
        last_secs: last,
        headway_secs: None,
    };
    let trimmed = apply(&net, vec![span(None, Some(9 * H))]);
    assert_eq!(starts(&trimmed, "10-weekday"), [28_800, 30_600]);
    // The early edge's gap is 30 min, the lower median of 30 min and 17 h.
    let earlier = apply(&net, vec![span(Some(7 * H), None)]);
    assert_eq!(
        starts(&earlier, "10-weekday"),
        [25_200, 27_000, 28_800, 30_600, 91_800]
    );
    let refuse = |e| refused(&net, e);
    assert!(refuse(span(Some(26 * H), None)).contains("leaves this direction no trip"));
    // A bound is the new last departure, in even steps nearest the edge
    // headway. 25:40 is within half a headway of 25:30, so it adds no trip.
    let later = apply(&net, vec![span(None, Some(26 * H))]);
    assert_eq!(
        starts(&later, "10-weekday"),
        [28_800, 30_600, 91_800, 93_600]
    );
    assert_eq!(
        starts(&apply(&net, vec![span(None, Some(92_400))]), "10-weekday"),
        [28_800, 30_600, 91_800]
    );
}

#[test]
fn running_time_changes_only_inside_the_window() {
    let net = network();
    let adjust = |delta: i32| NetworkEdit::AdjustRunningTime {
        route: "10".into(),
        direction: None,
        service_type: "weekday".into(),
        from_secs: 7 * H,
        to_secs: 9 * H,
        delta_secs: delta,
    };
    let slower = apply(&net, vec![adjust(120)]);
    assert_eq!(
        slower.patterns, net.patterns,
        "the profiles are a production tool, untouched"
    );
    let day = materialize(
        &slower,
        transit_date::Date::from_ymd_opt(2026, 10, 27).unwrap(),
    )
    .unwrap();
    let end = |start: u32| {
        day.iter()
            .find(|t| t.start_secs == start)
            .unwrap()
            .stop_times
            .last()
            .unwrap()
            .arrival_secs
            - start
    };
    assert_eq!(
        (end(28_800), end(91_800)),
        (720, 600),
        "the 08:00 trip runs longer, the 25:30 does not"
    );
    assert!(
        validation_issues(&slower).is_empty(),
        "12 min + 15 min layover still makes 08:30"
    );
    let late = apply(&net, vec![adjust(900)]);
    assert!(!validation_issues(&late).is_empty());
    assert!(refused(&net, adjust(-600)).contains("leaves no running time"));
}

#[test]
fn a_stop_change_reaches_every_pattern_of_the_direction() {
    let net = network();
    let mut variant = net.patterns["10-inbound"].clone();
    variant.stops[2].stop = "B".into();
    variant.stops.truncate(2);
    variant.stops[1].timepoint = true;
    let mut d = net.stops["B"].clone();
    d.name = "Main & 2½".into();
    let two = apply(
        &net,
        vec![
            NetworkEdit::UpsertStop {
                id: "D".into(),
                stop: d,
            },
            NetworkEdit::UpsertPattern {
                id: "10-short".into(),
                pattern: variant,
            },
        ],
    );
    let inserted = apply(
        &two,
        vec![NetworkEdit::InsertDirectionStop {
            route: "10".into(),
            direction: None,
            stop: "D".into(),
            after: "A".into(),
        }],
    );
    let visits = |n: &Network, p: &str| {
        n.patterns[p]
            .stops
            .iter()
            .map(|s| s.stop.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(visits(&inserted, "10-inbound"), ["A", "D", "B", "C"]);
    assert_eq!(visits(&inserted, "10-short"), ["A", "D", "B"]);
    assert!(
        inserted.trips().all(|(_, _, t)| t.times.len() == 4),
        "every trip gains the visit"
    );
    assert!(validation_issues(&inserted).is_empty());

    let removed = apply(
        &inserted,
        vec![NetworkEdit::RemoveDirectionStop {
            route: "10".into(),
            direction: None,
            stop: "D".into(),
        }],
    );
    assert_eq!(visits(&removed, "10-inbound"), ["A", "B", "C"]);
    assert_eq!(
        removed.pattern_trips, net.pattern_trips,
        "and loses it again"
    );
    let refusal = refused(
        &inserted,
        NetworkEdit::RemoveDirectionStop {
            route: "10".into(),
            direction: None,
            stop: "B".into(),
        },
    );
    assert!(refusal.contains("ends pattern 10-short"), "{refusal}");
    let placed = apply(
        &net,
        vec![NetworkEdit::InsertPatternStop {
            pattern: "10-inbound".into(),
            stop: "C".into(),
            index: Some(1),
        }],
    );
    assert_eq!(visits(&placed, "10-inbound"), ["A", "C", "B", "C"]);
    assert!(refused(
        &net,
        NetworkEdit::InsertPatternStop {
            pattern: "10-inbound".into(),
            stop: "C".into(),
            index: Some(0)
        }
    )
    .contains("endpoints stay"));
}

#[test]
fn removing_a_block_trip_keeps_the_other_layovers_where_they_were() {
    let net = network();
    let late = TripRef {
        trip: "10-weekday@91800".into(),
        extra: Default::default(),
    };
    let three = apply(
        &net,
        vec![
            NetworkEdit::AddBlockTrip {
                block: "400".into(),
                trip: late,
            },
            NetworkEdit::SetLayover {
                block: "400".into(),
                after_trip: 1,
                secs: Some(600),
            },
        ],
    );
    assert_eq!(three.blocks["400"].layovers.len(), 2);
    let two = apply(
        &three,
        vec![NetworkEdit::RemoveBlockTrip {
            block: "400".into(),
            index: 0,
        }],
    );
    let layovers: Vec<_> = two.blocks["400"]
        .layovers
        .iter()
        .map(|l| (l.after_trip, l.secs))
        .collect();
    assert_eq!(layovers, [(0, 600)]);
}

#[test]
fn what_a_timetable_gesture_cannot_do_is_named() {
    let net = network();
    let mut bands = net.clone();
    bands.pattern_trips.get_mut("10-weekday").unwrap().trips[0].departs = Departs::Every {
        bands: vec![HeadwayBand {
            start_secs: 8 * H,
            end_secs: 9 * H,
            headway_secs: 900,
            extra: Default::default(),
        }],
        exact: true,
    };
    bands.blocks.clear();
    let next = apply(&bands, vec![headway(8 * H, 9 * H, 600)]);
    assert_eq!(
        next.trip("10-weekday@28800").map(|x| x.2),
        bands.trip("10-weekday@28800").map(|x| x.2)
    );
    assert_eq!(next.pattern_trips["10-weekday"].trips.len(), 1 + 6 + 1);
    let elsewhere = NetworkEdit::SetHeadway {
        route: "10".into(),
        direction: Some("out".into()),
        service_type: "weekday".into(),
        from_secs: 0,
        to_secs: H,
        headway_secs: 600,
        pattern: None,
    };
    assert!(refused(&net, elsewhere).contains("has no direction out"));
    assert!(refused(&net, headway(9 * H, 8 * H, 600)).contains("window"));
    let late = NetworkEdit::SetSpan {
        route: "10".into(),
        direction: None,
        service_type: "weekday".into(),
        first_secs: None,
        last_secs: Some(31 * H),
        headway_secs: Some(H),
    };
    assert!(refused(&net, late).contains("has no timing profile to time a trip"));
}
