//! The diversion editor's laws, over the committed planner network
//! plus a loop pattern:
//!
//! 1. the default pattern is the route's longest; a named pattern of
//!    the route wins; a foreign one falls back;
//! 2. the stops strictly between the anchors are skipped unless
//!    served — new anchors never leave stale skips;
//! 3. an anchor revisited inside the span is never skipped;
//! 4. a draft that is not ready answers problems, never an error, and
//!    the record refuses it naming them;
//! 5. the traced path flattens with duplicates dropped, and a record
//!    carries no detour below two points;
//! 6. the list names what the network names, ids where it cannot.

use transit_dispatch::{diversion_of, diversion_rows, draft_view, DiversionDraft, DraftStopRole};
use transit_sync::{EditRecord, EditStatus};
use transit_network::{Network, PatternStop};
use transit_sync::DetourPoint;

fn network() -> Network {
    let mut net: Network = serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap();
    // A loop out and back: A B C B A — the route's longest pattern.
    let mut loop_pattern = net.patterns["10-inbound"].clone();
    loop_pattern.name = "10 loop".into();
    loop_pattern.stops = ["A", "B", "C", "B", "A"]
        .iter()
        .enumerate()
        .map(|(i, s)| PatternStop { stop: s.to_string(), timepoint: false, distance_m: i as f64 * 400.0 })
        .collect();
    loop_pattern.stops[0].timepoint = true;
    loop_pattern.stops[4].timepoint = true;
    net.patterns.insert("10-loop".into(), loop_pattern);
    net
}

fn draft() -> DiversionDraft {
    DiversionDraft {
        route_id: "10".into(),
        pattern: None,
        first_kept_stop: None,
        last_kept_stop: None,
        served_stops: vec![],
        slices: vec![],
        added_mins: 5.0,
        start_date: "2026-10-23".into(),
        end_date: "2026-10-23".into(),
    }
}

fn pt(lat: f64, lon: f64) -> DetourPoint {
    DetourPoint { lat, lon }
}

#[test]
fn the_default_pattern_is_the_longest() {
    let net = network();
    let view = draft_view(&net, &draft());
    assert_eq!(view.patterns.len(), 2);
    assert_eq!(view.pattern.as_deref(), Some("10-loop"));
    assert_eq!(view.stops.len(), 5);
    assert_eq!(view.alignment.len(), 5, "stop-to-stop without an authored path");
    let named = draft_view(&net, &DiversionDraft { pattern: Some("10-inbound".into()), ..draft() });
    assert_eq!(named.pattern.as_deref(), Some("10-inbound"));
    let foreign = draft_view(&net, &DiversionDraft { pattern: Some("nope".into()), ..draft() });
    assert_eq!(foreign.pattern.as_deref(), Some("10-loop"));
    assert_eq!(view.problems, vec!["pick the first stop the diversion keeps"]);
}

#[test]
fn the_interior_is_skipped_unless_served() {
    let net = network();
    let d = DiversionDraft {
        pattern: Some("10-inbound".into()),
        first_kept_stop: Some("A".into()),
        last_kept_stop: Some("C".into()),
        ..draft()
    };
    let view = draft_view(&net, &d);
    assert!(view.problems.is_empty(), "{:?}", view.problems);
    let roles: Vec<_> = view.stops.iter().map(|s| s.role).collect();
    assert_eq!(roles, vec![DraftStopRole::FirstKept, DraftStopRole::Interior, DraftStopRole::LastKept]);
    assert_eq!(view.skipped_stops, vec!["B"]);
    let served = draft_view(&net, &DiversionDraft { served_stops: vec!["B".into()], ..d.clone() });
    assert!(served.skipped_stops.is_empty(), "serving every interior stop skips none");
    // New anchors: a served stop outside the span is simply ignored.
    let moved = draft_view(&net, &DiversionDraft { last_kept_stop: Some("B".into()), served_stops: vec!["C".into()], ..d });
    assert!(moved.skipped_stops.is_empty());
}

#[test]
fn a_revisited_anchor_is_never_skipped() {
    let net = network();
    // On the loop, A … C spans B once; B … A (from the first B) spans
    // C and the second B — B is an anchor, only C is skipped.
    let d = DiversionDraft { first_kept_stop: Some("B".into()), last_kept_stop: Some("A".into()), ..draft() };
    let view = draft_view(&net, &d);
    assert!(view.problems.is_empty(), "{:?}", view.problems);
    assert_eq!(view.skipped_stops, vec!["C"]);
    let indexes: Vec<_> = view.stops.iter().filter(|s| s.role == DraftStopRole::Interior).map(|s| s.index).collect();
    assert_eq!(indexes, vec![2, 3]);
}

#[test]
fn an_unready_draft_answers_problems_and_the_record_refuses() {
    let net = network();
    let bad = DiversionDraft {
        pattern: Some("10-inbound".into()),
        first_kept_stop: Some("C".into()),
        last_kept_stop: Some("A".into()),
        start_date: "2026-10-24".into(),
        ..draft()
    };
    let view = draft_view(&net, &bad);
    assert_eq!(view.problems.len(), 2, "{:?}", view.problems);
    let err = diversion_of(&net, &bad, "d1").unwrap_err();
    assert!(err.contains("not after the first kept stop") && err.contains("start date"), "{err}");
    let unknown = draft_view(&net, &DiversionDraft { route_id: "99".into(), ..draft() });
    assert_eq!(unknown.problems[0], "unknown route 99");
}

#[test]
fn the_trace_flattens_and_short_traces_are_skip_only() {
    let net = network();
    let d = DiversionDraft {
        pattern: Some("10-inbound".into()),
        first_kept_stop: Some("A".into()),
        last_kept_stop: Some("C".into()),
        slices: vec![vec![pt(45.0, -122.0)], vec![pt(45.0, -122.0), pt(45.001, -122.0)]],
        added_mins: 2.5,
        ..draft()
    };
    let view = draft_view(&net, &d);
    assert_eq!(view.detour.len(), 2, "the repeated endpoint is dropped");
    assert!((view.detour_length_m - 110.54).abs() < 0.01);
    let record = diversion_of(&net, &d, "d1").unwrap();
    assert_eq!(record.added_secs, 150);
    assert_eq!(record.skipped_stops, vec!["B"]);
    assert_eq!(record.detour.as_ref().map(Vec::len), Some(2));
    let one = DiversionDraft { slices: vec![vec![pt(45.0, -122.0)]], ..d };
    assert!(diversion_of(&net, &one, "d1").unwrap().detour.is_none());
}

#[test]
fn the_list_names_what_the_network_names() {
    let net = network();
    let d = DiversionDraft {
        pattern: Some("10-inbound".into()),
        first_kept_stop: Some("A".into()),
        last_kept_stop: Some("C".into()),
        ..draft()
    };
    let mut gone = diversion_of(&net, &d, "d2").unwrap();
    gone.first_kept_stop = "Z".into();
    let records = vec![
        EditRecord { id: "e1".into(), diversion: diversion_of(&net, &d, "d1").unwrap(), status: EditStatus::Draft, rev: 1, updated_at: 1 },
        EditRecord { id: "e2".into(), diversion: gone, status: EditStatus::Retired, rev: 2, updated_at: 2 },
    ];
    let rows = diversion_rows(&net, &records);
    assert_eq!((rows[0].first_kept_name.as_str(), rows[0].skipped_names.clone()), ("Main & 1st", vec!["Main & 2nd".to_string()]));
    assert!(rows[0].in_force && !rows[1].in_force);
    assert_eq!(rows[1].first_kept_name, "Z", "an id the network lacks shows as itself");
}
