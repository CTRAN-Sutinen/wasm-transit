//! The cap-divert contract over native dispatch (OPS.md), on the committed
//! planner network. The draft and envelope properties are tested in
//! transit-dispatch's divert_test and in transit-sync. An older edit document
//! is refused and names the migration op, and a newer one names both versions
//! (docs/FORMATS.md, "Versions").

use cap_abi::{Base, Current};
use cap_divert::{dispatch, dispatch_bytes, dispatch_json, Request, Response};
use transit_dispatch::DiversionDraft;
use transit_network::Network;
use transit_sync::{EditDocument, EditRecord, EditStatus};

fn load() -> String {
    let network: Network = serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap();
    cap_abi::kernel::kind::<transit_network::Network>().register(network).unwrap().0
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

fn doc(records: Vec<EditRecord>) -> Current<EditDocument> {
    Current(EditDocument::new(records))
}

fn base(records: Vec<EditRecord>) -> Base<EditDocument> {
    Base(EditDocument::new(records))
}

fn propose(hash: &str, records: Vec<EditRecord>, draft: DiversionDraft, now_ms: u64) -> Result<Vec<EditRecord>, cap_abi::Refusal> {
    match dispatch(Request::Propose { network_hash: hash.into(), draft, edits: base(records), record_id: "e1".into(), now_ms })? {
        Response::Propose { edits } => Ok(edits.records),
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
    assert_eq!(records[0].diversion.as_ref().unwrap().id, "div-10-1000");
    assert_eq!(records[0].diversion.as_ref().unwrap().added_secs, 300);
    let Response::List { rows, .. } =
        dispatch(Request::List { network_hash: hash.clone(), edits: doc(records.clone()) }).unwrap()
    else {
        panic!("list")
    };
    assert_eq!((rows[0].route_name.as_str(), rows[0].skipped_names.len()), ("Route 10", 1));
    let again = propose(&hash, records, DiversionDraft { added_mins: 10.0, ..draft() }, 2_000).unwrap();
    assert_eq!((again[0].rev, again[0].diversion.as_ref().unwrap().id.as_str()), (2, "div-10-1000"), "the domain id is stable");
    let Response::Retire { edits } =
        dispatch(Request::Retire { edits: base(again), record_id: "e1".into(), now_ms: 3_000 }).unwrap()
    else {
        panic!("retire")
    };
    assert_eq!(edits.records[0].status, EditStatus::Retired);
    let Response::Restore { edits } = dispatch(Request::Restore { edits: Base(edits), record_id: "e1".into(), now_ms: 4_000 }).unwrap()
    else {
        panic!("restore")
    };
    assert_eq!((edits.records[0].status, edits.records[0].rev), (EditStatus::Draft, 4));
}

/// The answer is the next document itself, tagged.
#[test]
fn every_answer_is_the_tagged_document_in_this_version() {
    let hash = load();
    let request = serde_json::json!({
        "op": "propose", "network_hash": hash, "draft": draft(), "record_id": "e1", "now_ms": 1_000,
        "edits": {"kind": "edit_records", "schema_version": transit_sync::SCHEMA_VERSION, "records": []},
    });
    let answer: serde_json::Value = serde_json::from_str(&dispatch_json(&request.to_string())).unwrap();
    let edits = &answer["ok"]["edits"];
    assert_eq!((edits["kind"].as_str(), edits["schema_version"].as_u64()), (Some("edit_records"), Some(transit_sync::SCHEMA_VERSION as u64)), "{answer}");
    assert_eq!(edits["records"].as_array().map(Vec::len), Some(1), "{answer}");
}

#[test]
fn another_version_of_the_edit_document_is_refused_naming_both() {
    let hash = load();
    let retire = |edits: serde_json::Value| {
        dispatch_json(&serde_json::json!({"op": "retire", "edits": edits, "record_id": "e1", "now_ms": 1}).to_string())
    };
    // v1: the bare array an older build wrote.
    let older = retire(serde_json::json!([]));
    assert!(older.contains("edit_records v1") && older.contains("migrate.migrate"), "{older}");
    let newer = retire(serde_json::json!({"kind": "edit_records", "schema_version": 3, "records": []}));
    assert!(newer.contains("edit_records v3") && newer.contains("reads v2"), "{newer}");
    let list = dispatch_json(&serde_json::json!({"op": "list", "network_hash": hash, "edits": []}).to_string());
    assert!(list.contains("migrate.migrate"), "{list}");
}

#[test]
fn refusals_are_errors_never_panics() {
    let hash = load();
    let err = propose(&hash, vec![], DiversionDraft { first_kept_stop: None, ..draft() }, 1).unwrap_err();
    assert!(err.message.contains("first stop"), "{err}");
    assert!(dispatch(Request::Retire { edits: base(vec![]), record_id: "nope".into(), now_ms: 1 }).is_err());
    assert!(dispatch(Request::Draft { network_hash: "sha256:nope".into(), draft: draft() }).is_err());
    assert!(dispatch_json("{not json").contains("\"error\""));
    assert!(dispatch_json(r#"{"op":"teleport"}"#).contains("invalid request"));
    assert!(dispatch_bytes(1, &[]).contains("\"error\""));
}

#[test]
fn the_kernel_bulk_op_registers_the_same_identity_as_load_network() {
    // The bulk op parses the bare document with no request envelope and
    // answers what the JSON op answers.
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_divert::invoke(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_divert::invoke_bytes(<transit_network::Network as cap_abi::kernel::Load>::CODE, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_divert::invoke_bytes(<transit_network::Network as cap_abi::kernel::Load>::CODE, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}

#[test]
fn reports_say_what_each_record_amounts_to_on_the_network() {
    let network_hash = load();
    let record = |id: &str, route: &str| EditRecord {
        id: id.into(),
        diversion: Some(transit_sync::Diversion {
            id: format!("d-{id}"),
            route_id: route.into(),
            direction_id: None,
            first_kept_stop: "A".into(),
            last_kept_stop: "B".into(),
            skipped_stops: vec!["C".into()],
            detour: None,
            added_secs: 300,
            start_date: transit_date::Date::from_ymd_opt(2026, 10, 23).unwrap(),
            end_date: transit_date::Date::from_ymd_opt(2026, 10, 23).unwrap(),
            extra: Default::default(),
        }),
        cancel: None,
        status: EditStatus::Accepted,
        rev: 1,
        updated_at: 1,
        extra: Default::default(),
    };
    let records = vec![record("e1", "10"), record("e2", "ghost")];
    let Response::Reports { reports } = dispatch(Request::Reports { network_hash, edits: doc(records) }).unwrap() else {
        panic!("reports")
    };
    assert_eq!(reports.len(), 2);
    assert!(reports[0].valid && reports[0].affected_trips >= 1, "{reports:?}");
    assert!(!reports[1].valid, "an edit against an unknown route is reported invalid: {reports:?}");
    let err = dispatch(Request::Reports { network_hash: "sha256:missing".into(), edits: doc(vec![]) }).unwrap_err();
    assert!(err.message.contains("unknown network"), "{err}");
}

fn cancel(hash: &str, records: Vec<EditRecord>, start_secs: Option<u32>, date: &str, now_ms: u64) -> Result<Vec<EditRecord>, cap_abi::Refusal> {
    let request = Request::Cancel {
        network_hash: hash.into(),
        edits: base(records),
        record_id: "c1".into(),
        trip_id: "10-weekday@28800".into(),
        start_secs,
        date: date.into(),
        now_ms,
    };
    match dispatch(request)? {
        Response::Cancel { edits } => Ok(edits.records),
        _ => panic!("cancel"),
    }
}

#[test]
fn a_trip_cancel_walks_the_edit_document() {
    let hash = load();
    let diverted = propose(&hash, vec![], draft(), 1_000).unwrap();
    let records = cancel(&hash, diverted, None, "2026-10-23", 2_000).unwrap();
    assert_eq!(records.len(), 2);
    let c = records[1].cancel.as_ref().unwrap();
    assert_eq!((c.id.as_str(), c.trip_id.as_str(), records[1].status), ("cancel-10-weekday@28800-2000", "10-weekday@28800", EditStatus::Draft));
    assert!(records[1].diversion.is_none());

    let Response::List { rows, cancels } = dispatch(Request::List { network_hash: hash.clone(), edits: doc(records.clone()) }).unwrap() else {
        panic!("list")
    };
    assert_eq!((rows.len(), cancels.len()), (1, 1), "each payload lists in its own rows");
    assert_eq!((cancels[0].record_id.as_str(), cancels[0].start_secs, cancels[0].date.as_str()), ("c1", Some(28_800), "2026-10-23"));

    let Response::Reports { reports } = dispatch(Request::Reports { network_hash: hash.clone(), edits: doc(records.clone()) }).unwrap() else {
        panic!("reports")
    };
    assert_eq!((reports[1].valid, reports[1].affected_trips), (true, 1));

    let again = cancel(&hash, records, Some(28_800), "2026-10-23", 3_000).unwrap();
    assert_eq!((again[1].rev, again[1].cancel.as_ref().unwrap().id.as_str()), (2, "cancel-10-weekday@28800-2000"), "the domain id is stable");
    let Response::Retire { edits } = dispatch(Request::Retire { edits: base(again), record_id: "c1".into(), now_ms: 4_000 }).unwrap() else {
        panic!("retire")
    };
    assert_eq!(edits.records[1].status, EditStatus::Retired);

    let err = cancel(&hash, vec![], None, "2026-11-26", 1).unwrap_err();
    assert!(err.message.contains("does not run on 2026-11-26"), "Thanksgiving is removed: {err}");
    let err = cancel("sha256:missing", vec![], None, "2026-10-23", 1).unwrap_err();
    assert!(err.message.contains("unknown network"), "{err}");
}
