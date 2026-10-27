//! The timetable's gesture laws, over the committed planner network
//! (testdata/network.json: route 10's one pattern 10-inbound A·B·C in
//! no direction, 10-weekday at 08:00 / 08:30 / 25:30, block 400
//! chaining 08:00 → 08:30 with 15 min layover at C).
//!
//! 1. a trip that stays is retimed, never re-created: its blocks move
//!    with it and the runcut stays valid;
//! 2. a trip that goes leaves its blocks, and the layovers around it
//!    go too (the chain closes up, never mis-indexed);
//! 3. new trips land in the neighbour's pattern trips, unassigned;
//! 4. running time changes split profiles at the window, so trips
//!    outside it keep their times;
//! 5. a stop change applies to every pattern of the direction, by
//!    stop id, and endpoints stay;
//! 6. what a gesture cannot do is a named error.

use transit_network::{
    apply_edits, materialize, validation_issues, Departures, HeadwayBand, Network, NetworkEdit, NetworkWarning, TripRef,
};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap()
}

fn apply(net: &Network, edits: Vec<NetworkEdit>) -> Network {
    apply_edits(net, &edits).expect("the edits apply")
}

fn refused(net: &Network, edit: NetworkEdit) -> String {
    apply_edits(net, &[edit]).expect_err("the edit is refused").reason
}

fn starts(net: &Network, id: &str) -> Vec<u32> {
    match &net.pattern_trips[id].departures {
        Departures::ExactTimes { starts } => starts.clone(),
        Departures::HeadwayBands { .. } => panic!("exact times"),
    }
}

fn block_starts(net: &Network) -> Vec<u32> {
    net.blocks["400"].trips.iter().map(|t| t.start_secs).collect()
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
    assert_eq!(starts(&next, "10-weekday"), [28_800, 29_700, 30_600, 31_500, 91_800]);
    assert_eq!(block_starts(&next), [28_800, 30_600], "the runcut is untouched");
    assert_eq!(next.blocks["400"].layovers, net.blocks["400"].layovers);
    assert!(validation_issues(&next).is_empty());
    let unassigned = transit_network::validation_warnings(&next);
    assert!(unassigned.iter().any(|w| matches!(w, NetworkWarning::UnassignedTrips { trips: 3, .. })), "{unassigned:?}");
}

#[test]
fn a_new_clockface_moves_trips_with_their_blocks() {
    // 07:50 / 08:20 / 08:50: the 08:00 and 08:30 trips are nearest
    // the first two slots (ten minutes each), the third is new.
    let next = apply(&network(), vec![headway(7 * H + 3000, 9 * H, 1800)]);
    assert_eq!(starts(&next, "10-weekday"), [28_200, 30_000, 31_800, 91_800]);
    assert_eq!(block_starts(&next), [28_200, 30_000], "the block carries the moved trips");
    assert_eq!(next.blocks["400"].layovers.len(), 1, "the layover between them stays");
    assert!(validation_issues(&next).is_empty(), "{:?}", validation_issues(&next));
}

#[test]
fn a_lower_frequency_drops_trips_and_closes_the_chain() {
    let next = apply(&network(), vec![headway(8 * H, 9 * H, H)]);
    assert_eq!(starts(&next, "10-weekday"), [28_800, 91_800]);
    assert_eq!(block_starts(&next), [28_800]);
    assert!(next.blocks["400"].layovers.is_empty(), "the layover before the dropped trip goes with it");
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
    // The early edge's gap is 30 min (the lower median of 30 min and
    // 17 h), so 07:00 is reached in two trips.
    let earlier = apply(&net, vec![span(Some(7 * H), None)]);
    assert_eq!(starts(&earlier, "10-weekday"), [25_200, 27_000, 28_800, 30_600, 91_800]);
    let refuse = |e| refused(&net, e);
    assert!(refuse(span(Some(26 * H), None)).contains("leaves this direction no trip"));
    // A bound is the new last departure, in even steps nearest the
    // edge headway (17 h → 26:00 at 30 min: 25:30 is the edge, 26:00
    // the one step; 25:40 is within half a headway — no trip).
    let later = apply(&net, vec![span(None, Some(26 * H))]);
    assert_eq!(starts(&later, "10-weekday"), [28_800, 30_600, 91_800, 93_600]);
    assert_eq!(starts(&apply(&net, vec![span(None, Some(92_400))]), "10-weekday"), [28_800, 30_600, 91_800]);
}

#[test]
fn running_time_changes_only_inside_the_window() {
    let net = network();
    let adjust = |delta: i32| NetworkEdit::AdjustRunningTime {
        route: "10".into(),
        direction: None,
        from_secs: 7 * H,
        to_secs: 9 * H,
        delta_secs: delta,
    };
    let slower = apply(&net, vec![adjust(120)]);
    let timing: Vec<_> = slower.patterns["10-inbound"].timing.iter().map(|t| (t.start_secs, t.end_secs, t.at_timepoints.clone())).collect();
    assert_eq!(timing, [(0, 7 * H, vec![0, 600]), (7 * H, 9 * H, vec![0, 720]), (9 * H, 108_000, vec![0, 600])]);
    let day = materialize(&slower, chrono::NaiveDate::from_ymd_opt(2026, 10, 27).unwrap()).unwrap();
    let end = |start: u32| day.iter().find(|t| t.start_secs == start).unwrap().stop_times.last().unwrap().arrival_secs - start;
    assert_eq!((end(28_800), end(91_800)), (720, 600), "the 08:00 trip runs longer, the 25:30 does not");
    assert!(validation_issues(&slower).is_empty(), "12 min + 15 min layover still makes 08:30");
    // Too slow for the block's layover: validation names it.
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
            NetworkEdit::UpsertStop { id: "D".into(), stop: d },
            NetworkEdit::UpsertPattern { id: "10-short".into(), pattern: variant },
        ],
    );
    let inserted = apply(
        &two,
        vec![NetworkEdit::InsertDirectionStop { route: "10".into(), direction: None, stop: "D".into(), after: "A".into() }],
    );
    let visits = |n: &Network, p: &str| n.patterns[p].stops.iter().map(|s| s.stop.clone()).collect::<Vec<_>>();
    assert_eq!(visits(&inserted, "10-inbound"), ["A", "D", "B", "C"]);
    assert_eq!(visits(&inserted, "10-short"), ["A", "D", "B"]);
    assert!(validation_issues(&inserted).is_empty());

    let removed = apply(
        &inserted,
        vec![NetworkEdit::RemoveDirectionStop { route: "10".into(), direction: None, stop: "D".into() }],
    );
    assert_eq!(visits(&removed, "10-inbound"), ["A", "B", "C"]);
    // B ends 10-short: endpoints stay, on every pattern.
    let refusal = refused(&inserted, NetworkEdit::RemoveDirectionStop { route: "10".into(), direction: None, stop: "B".into() });
    assert!(refusal.contains("ends pattern 10-short"), "{refusal}");
    // A pattern stop can be placed anywhere in the middle now.
    let placed = apply(&net, vec![NetworkEdit::InsertPatternStop { pattern: "10-inbound".into(), stop: "C".into(), index: Some(1) }]);
    assert_eq!(visits(&placed, "10-inbound"), ["A", "C", "B", "C"]);
    assert!(refused(&net, NetworkEdit::InsertPatternStop { pattern: "10-inbound".into(), stop: "C".into(), index: Some(0) }).contains("endpoints stay"));
}

#[test]
fn removing_a_block_trip_keeps_the_other_layovers_where_they_were() {
    let net = network();
    let late = TripRef { pattern_trips: "10-weekday".into(), start_secs: 91_800 };
    let three = apply(
        &net,
        vec![
            NetworkEdit::AddBlockTrip { block: "400".into(), trip: late },
            NetworkEdit::SetLayover { block: "400".into(), after_trip: 1, secs: Some(600) },
        ],
    );
    assert_eq!(three.blocks["400"].layovers.len(), 2);
    let two = apply(&three, vec![NetworkEdit::RemoveBlockTrip { block: "400".into(), index: 0 }]);
    // The 08:30 → 25:30 layover (600 s) is now after trip 0; the
    // 08:00 → 08:30 one went with the 08:00 trip.
    let layovers: Vec<_> = two.blocks["400"].layovers.iter().map(|l| (l.after_trip, l.secs)).collect();
    assert_eq!(layovers, [(0, 600)]);
}

#[test]
fn what_a_timetable_gesture_cannot_do_is_named() {
    let net = network();
    let mut bands = net.clone();
    bands.pattern_trips.get_mut("10-weekday").unwrap().departures = Departures::HeadwayBands {
        bands: vec![HeadwayBand { start_secs: 8 * H, end_secs: 9 * H, headway_secs: 900 }],
    };
    bands.blocks.clear();
    assert!(refused(&bands, headway(8 * H, 9 * H, 600)).contains("headway bands"));
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
}
