//! The authoring laws — `apply_edits` over the committed planner
//! network (testdata/network.json: stops A/B/C, route 10, one
//! pattern, one weekday schedule, block 400 with a layover at C).
//!
//! 1. the input document is never mutated; no edits is identity;
//! 2. a change is atomic — one edit that cannot apply rejects the
//!    whole list, naming its position;
//! 3. removal refuses while anything references the entity, and
//!    succeeds once the references are gone (no dangling documents);
//! 4. upserts are the planner's verbs: a changed trip list changes
//!    the materialized day, a plan frame stamps the pick.

use chrono::NaiveDate;
use transit_network::{
    apply_edits, materialize, validation_issues, Departs, Network, NetworkEdit, PlanMeta,
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
    // The weekday trips gain a midday trip, through the model's own
    // vocabulary.
    let mut pattern_trips = net.pattern_trips["10-weekday"].clone();
    let mut midday = pattern_trips.trips[0].clone();
    midday.id = "midday".into();
    midday.departs = Departs::At { start_secs: 43_200 };
    pattern_trips.trips.push(midday);
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

    // An upsert that drops a block's trip is APPLIED (a document
    // mid-edit may be invalid) and validation names the break.
    let mut stranded = net.pattern_trips["10-weekday"].clone();
    stranded.trips.remove(0);
    let broken = apply_edits(&net, &[NetworkEdit::UpsertPatternTrips { id: "10-weekday".into(), pattern_trips: stranded }]).unwrap();
    assert!(
        validation_issues(&broken)
            .iter()
            .any(|i| matches!(i, transit_network::NetworkError::UnknownBlockTrip { trip, .. } if trip == "10-weekday@28800")),
        "the stranded block trip is reported"
    );
}

#[test]
fn a_retime_keeps_the_trip_and_its_runcut() {
    // Block 400 carries the 08:00 and 08:30 trips by id. Moving the
    // 08:00 trip to 08:05 keeps its id, so the block still runs it and
    // the layover still fits.
    let net = network();
    let next = apply_edits(&net, &[NetworkEdit::RetimeTrip { trip: "10-weekday@28800".into(), to_secs: 29_100 }]).unwrap();
    let (_, _, moved) = next.trip("10-weekday@28800").expect("same id");
    assert_eq!(moved.departs, Departs::At { start_secs: 29_100 });
    assert_eq!(next.blocks, net.blocks);
    assert!(validation_issues(&next).is_empty(), "{:?}", validation_issues(&next));
    // Past the next trip's start, the block cannot run it — reported.
    let late = apply_edits(&net, &[NetworkEdit::RetimeTrip { trip: "10-weekday@28800".into(), to_secs: 30_300 }]).unwrap();
    assert!(!validation_issues(&late).is_empty());

    let refuse = |edit: NetworkEdit, needle: &str| {
        let err = apply_edits(&net, &[edit]).unwrap_err();
        assert!(err.reason.contains(needle), "{err}");
    };
    refuse(NetworkEdit::RetimeTrip { trip: "ghost".into(), to_secs: 2 }, "unknown trip ghost");
}

#[test]
fn trips_are_added_and_removed_by_id() {
    let net = network();
    let next = apply_edits(
        &net,
        &[NetworkEdit::AddTrip { pattern_trips: "10-weekday".into(), id: None, start_secs: 43_200 }],
    )
    .unwrap();
    let (_, _, added) = next.trip("10-weekday@43200").expect("the minted id");
    assert_eq!(added.times.len(), 3, "timed at every stop by the profile");
    assert_eq!(added.run_secs(), Some(600));
    assert!(validation_issues(&next).is_empty());
    let refused = apply_edits(
        &net,
        &[NetworkEdit::AddTrip { pattern_trips: "10-weekday".into(), id: Some("10-weekday@28800".into()), start_secs: 1 }],
    )
    .unwrap_err();
    assert!(refused.reason.contains("already exists"), "{refused}");

    // Removing a blocked trip closes the block up; its layover goes.
    let next = apply_edits(&net, &[NetworkEdit::RemoveTrip { trip: "10-weekday@28800".into() }]).unwrap();
    assert!(next.trip("10-weekday@28800").is_none());
    assert_eq!(next.blocks["400"].trips.len(), 1);
    assert!(next.blocks["400"].layovers.is_empty());
    assert!(validation_issues(&next).is_empty(), "{:?}", validation_issues(&next));
}
