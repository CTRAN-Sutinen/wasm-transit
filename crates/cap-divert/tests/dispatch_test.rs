//! Native dispatch tests for the diversion-authoring capability — the
//! seam without wasm, over the committed planner network. The laws
//! (the draft and envelope semantics live in transit-dispatch's
//! divert_test and transit-sync):
//!
//! 1. a draft answers its view, problems included, never an error;
//! 2. propose → retire → restore walks the edit document's states,
//!    the domain id stable across re-proposals;
//! 3. an unready draft, an unknown record, an unknown hash, and
//!    garbage are errors, never panics.

use cap_divert::{dispatch, dispatch_bytes, dispatch_json, Request, Response};
use transit_dispatch::DiversionDraft;
use transit_network::Network;
use transit_sync::{EditRecord, EditStatus};

fn load() -> String {
    let network: Network = serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap();
    match dispatch(Request::LoadNetwork { network }).unwrap() {
        Response::LoadNetwork { network_hash } => network_hash,
        _ => panic!("load_network"),
    }
}

fn draft() -> DiversionDraft {
    DiversionDraft {
        route_id: "10".into(),
        pattern: None,
        first_kept_stop: Some("A".into()),
        last_kept_stop: Some("C".into()),
        served_stops: vec![],
        slices: vec![],
        added_mins: 5.0,
        start_date: "2026-10-23".into(),
        end_date: "2026-10-23".into(),
    }
}

fn propose(hash: &str, records: Vec<EditRecord>, draft: DiversionDraft, now_ms: u64) -> Result<Vec<EditRecord>, String> {
    match dispatch(Request::Propose { network_hash: hash.into(), draft, records, record_id: "e1".into(), now_ms })? {
        Response::Propose { records } => Ok(records),
        _ => panic!("propose"),
    }
}

#[test]
fn a_draft_answers_its_view() {
    let hash = load();
    let Response::Draft { view } = dispatch(Request::Draft { network_hash: hash.clone(), draft: draft() }).unwrap() else {
        panic!("draft")
    };
    assert_eq!(view.pattern.as_deref(), Some("10-inbound"));
    assert_eq!(view.skipped_stops, vec!["B"]);
    assert!(view.problems.is_empty());
    let Response::Draft { view } =
        dispatch(Request::Draft { network_hash: hash, draft: DiversionDraft { last_kept_stop: None, ..draft() } }).unwrap()
    else {
        panic!("draft")
    };
    assert_eq!(view.problems, vec!["pick the last stop the diversion keeps"]);
}

#[test]
fn the_edit_document_walks_propose_retire_restore() {
    let hash = load();
    let records = propose(&hash, vec![], draft(), 1_000).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].diversion.id, "div-10-1000");
    assert_eq!(records[0].diversion.added_secs, 300);
    let Response::List { rows } =
        dispatch(Request::List { network_hash: hash.clone(), records: records.clone() }).unwrap()
    else {
        panic!("list")
    };
    assert_eq!((rows[0].route_name.as_str(), rows[0].skipped_names.len()), ("Route 10", 1));
    let again = propose(&hash, records, DiversionDraft { added_mins: 10.0, ..draft() }, 2_000).unwrap();
    assert_eq!((again[0].rev, again[0].diversion.id.as_str()), (2, "div-10-1000"), "the domain id is stable");
    let Response::Retire { records } =
        dispatch(Request::Retire { records: again, record_id: "e1".into(), now_ms: 3_000 }).unwrap()
    else {
        panic!("retire")
    };
    assert_eq!(records[0].status, EditStatus::Retired);
    let Response::Restore { records } =
        dispatch(Request::Restore { records, record_id: "e1".into(), now_ms: 4_000 }).unwrap()
    else {
        panic!("restore")
    };
    assert_eq!((records[0].status, records[0].rev), (EditStatus::Draft, 4));
    let Response::Sanitize { records: kept, dropped } =
        dispatch(Request::Sanitize { records: serde_json::json!([records[0], {"id": 1}]) }).unwrap()
    else {
        panic!("sanitize")
    };
    assert_eq!((kept.len(), dropped), (1, 1));
}

#[test]
fn refusals_are_errors_never_panics() {
    let hash = load();
    let err = propose(&hash, vec![], DiversionDraft { first_kept_stop: None, ..draft() }, 1).unwrap_err();
    assert!(err.contains("first stop"), "{err}");
    assert!(dispatch(Request::Retire { records: vec![], record_id: "nope".into(), now_ms: 1 }).is_err());
    assert!(dispatch(Request::Draft { network_hash: "sha256:nope".into(), draft: draft() }).is_err());
    assert!(dispatch_json("{not json").contains("\"error\""));
    assert!(dispatch_json(r#"{"op":"teleport"}"#).contains("invalid request"));
    assert!(dispatch_bytes(1, &[]).contains("\"error\""));
}
