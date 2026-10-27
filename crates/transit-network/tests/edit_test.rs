//! The authoring laws — `apply_edits` over the committed planner
//! network (testdata/network.json: stops A/B/C, route 10, one
//! pattern, one weekday schedule, block 400 with a layover at C).
//!
//! 1. the input document is never mutated; no edits is identity;
//! 2. a change is atomic — one edit that cannot apply rejects the
//!    whole list, naming its position;
//! 3. removal refuses while anything references the entity, and
//!    succeeds once the references are gone (no dangling documents);
//! 4. upserts are the planner's verbs: a retimed schedule changes
//!    the materialized day, a plan frame stamps the pick.

use chrono::NaiveDate;
use transit_network::{
    apply_edits, materialize, validation_issues, Network, NetworkEdit, PlanMeta, Departures,
};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).expect("testdata network parses")
}

fn tuesday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 27).unwrap()
}

fn remove(edit: fn(String) -> NetworkEdit, id: &str) -> NetworkEdit {
    edit(id.to_string())
}

#[test]
fn the_input_is_never_mutated() {
    let net = network();
    let before = net.clone();
    let next = apply_edits(&net, &[NetworkEdit::SetName { name: "renamed".into() }]).unwrap();
    assert_eq!(net, before, "the last document is untouched");
    assert_eq!(next.name, "renamed");
    assert_eq!(apply_edits(&net, &[]).unwrap(), net, "no edits answer the same document");
}

#[test]
fn a_change_is_atomic() {
    let net = network();
    let edits = [
        NetworkEdit::SetName { name: "half-applied".into() },
        remove(|id| NetworkEdit::RemoveRoute { id }, "ghost"),
    ];
    let err = apply_edits(&net, &edits).unwrap_err();
    assert_eq!(err.index, 1, "the failing edit is named by position");
    assert!(err.reason.contains("unknown route ghost"), "{err}");
    // Nothing of the change escaped: the caller still holds `net`.
    assert_eq!(net.name, "Planner Network");
}

#[test]
fn removal_refuses_while_referenced() {
    let net = network();
    let refused = |edit: NetworkEdit, needle: &str| {
        let err = apply_edits(&net, &[edit]).unwrap_err();
        assert!(err.reason.contains(needle), "{err}");
    };
    refused(remove(|id| NetworkEdit::RemoveStop { id }, "A"), "visited by pattern 10-inbound");
    refused(remove(|id| NetworkEdit::RemoveRoute { id }, "10"), "carries pattern 10-inbound");
    refused(remove(|id| NetworkEdit::RemovePattern { id }, "10-inbound"), "run by pattern trips 10-weekday");
    refused(remove(|id| NetworkEdit::RemoveServiceType { id }, "weekday"), "governs pattern trips 10-weekday");
    refused(remove(|id| NetworkEdit::RemovePatternTrips { id }, "10-weekday"), "trips in block 400");

    // Dependents first, then the entity: the whole route comes out
    // and the document stays valid (no dangling reference anywhere).
    let teardown: Vec<NetworkEdit> = vec![
        remove(|id| NetworkEdit::RemoveBlock { id }, "400"),
        remove(|id| NetworkEdit::RemovePatternTrips { id }, "10-weekday"),
        remove(|id| NetworkEdit::RemovePattern { id }, "10-inbound"),
        remove(|id| NetworkEdit::RemoveRoute { id }, "10"),
        remove(|id| NetworkEdit::RemoveServiceType { id }, "weekday"),
        remove(|id| NetworkEdit::RemoveStop { id }, "B"),
    ];
    let empty = apply_edits(&net, &teardown).unwrap();
    assert!(empty.routes.is_empty() && empty.pattern_trips.is_empty());
    assert_eq!(empty.stops.len(), 2);
    assert!(validation_issues(&empty).is_empty(), "{:?}", validation_issues(&empty));
}

#[test]
fn upserts_are_the_planners_verbs() {
    let net = network();
    // Retime: the weekday schedule gains a midday trip (the 006-style
    // frequency change, through the model's own vocabulary).
    let mut pattern_trips = net.pattern_trips["10-weekday"].clone();
    pattern_trips.departures = Departures::ExactTimes { starts: vec![28_800, 30_600, 43_200, 91_800] };
    let next = apply_edits(
        &net,
        &[
            NetworkEdit::UpsertPatternTrips { id: "10-weekday".into(), pattern_trips },
            NetworkEdit::SetPlan {
                plan: PlanMeta { id: "spring".into(), name: "Spring".into(), effective_from: Some(tuesday()) },
            },
        ],
    )
    .unwrap();
    assert_eq!(materialize(&net, tuesday()).unwrap().len(), 3);
    assert_eq!(materialize(&next, tuesday()).unwrap().len(), 4, "the added trip materializes");
    assert_eq!(next.plan.id, "spring", "the pick frame is stamped");
    assert!(validation_issues(&next).is_empty());

    // A retime that strands a block trip is APPLIED (a document
    // mid-edit may be invalid) and validation names the break.
    let mut stranded = net.pattern_trips["10-weekday"].clone();
    stranded.departures = Departures::ExactTimes { starts: vec![30_600, 91_800] };
    let broken = apply_edits(&net, &[NetworkEdit::UpsertPatternTrips { id: "10-weekday".into(), pattern_trips: stranded }]).unwrap();
    assert!(
        validation_issues(&broken)
            .iter()
            .any(|i| matches!(i, transit_network::NetworkError::UnknownBlockTrip { start_secs: 28_800, .. })),
        "the stranded block trip is reported"
    );
}

#[test]
fn a_retime_carries_the_runcut_along() {
    // Block 400 carries 08:00 and 08:30. Moving the 08:00 trip to
    // 08:05 moves the block's reference with it: the document stays
    // valid (QAF-1's check never fires), the layover still fits.
    let net = network();
    let next = apply_edits(&net, &[NetworkEdit::RetimeTrip { pattern_trips: "10-weekday".into(), from_secs: 28_800, to_secs: 29_100 }]).unwrap();
    assert_eq!(next.blocks["400"].trips[0].start_secs, 29_100);
    assert!(validation_issues(&next).is_empty(), "{:?}", validation_issues(&next));
    // The same move as a bare upsert strands the block — reported.
    let mut pattern_trips = net.pattern_trips["10-weekday"].clone();
    pattern_trips.departures = Departures::ExactTimes { starts: vec![29_100, 30_600, 91_800] };
    let stranded = apply_edits(&net, &[NetworkEdit::UpsertPatternTrips { id: "10-weekday".into(), pattern_trips }]).unwrap();
    assert!(!validation_issues(&stranded).is_empty());

    // Refusals: no such trip, a collision, a headway schedule.
    let refuse = |edit: NetworkEdit, needle: &str| {
        let err = apply_edits(&net, &[edit]).unwrap_err();
        assert!(err.reason.contains(needle), "{err}");
    };
    refuse(NetworkEdit::RetimeTrip { pattern_trips: "10-weekday".into(), from_secs: 1, to_secs: 2 }, "runs no trip at 1");
    refuse(NetworkEdit::RetimeTrip { pattern_trips: "10-weekday".into(), from_secs: 28_800, to_secs: 30_600 }, "already runs a trip");
    refuse(NetworkEdit::RetimeTrip { pattern_trips: "ghost".into(), from_secs: 1, to_secs: 2 }, "unknown pattern trips");
}
