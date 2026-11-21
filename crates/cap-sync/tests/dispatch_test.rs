//! Native dispatch tests for the sync capability — the seam without
//! wasm, over the e2e fixture's edit-record shape. The laws (the
//! merge's own laws are property-tested in transit-sync; here the
//! seam pins them through the module):
//!
//! 1. the union of ids, sorted; LWW per record id — the newer
//!    `updated_at` wins, a tombstone propagates over a stale live
//!    record;
//! 2. drafts submit (rev bumped, the host clock stamped); records
//!    already past draft pass through untouched;
//! 3. the answer is byte-deterministic — the same pair merges to
//!    the same document in any input order (client and server are
//!    roles, not order);
//! 4. garbage and bulk bytes are honest errors, never panics;
//! 5. the receive path clamps the client clock to `now_ms` (D-56),
//!    and the answer carries the canonical document bytes plus their
//!    hash, so a host stores exact bytes and never mints identity;
//! 6. `read_edits` migrates a received document at receive (defaults
//!    fill what age removed, malformed records are counted, never
//!    wedging it), and `compact_edits` is the D-57 tombstone commit;
//! 7. the disruptions log's records (D-190, D-197): the document's
//!    records, folded by the log and assembled again, are the merged
//!    document, byte for byte; a write that changes nothing changes no
//!    record; the document's own fields survive; a record in another
//!    version is refused; a revert writes back each record stamped to
//!    win the merge, and retires a record the reverted write created
//!    (D-193).

use cap_sync::{dispatch, dispatch_bytes, dispatch_json, Request, Response};
use transit_sync::{Diversion, EditRecord, EditStatus};

const DATE: &str = "2026-10-23";

fn record(id: &str, status: EditStatus, rev: u64, updated_at: u64) -> EditRecord {
    EditRecord {
        id: id.into(),
        diversion: Some(Diversion {
            id: format!("d-{id}"),
            route_id: "10".into(),
            direction_id: None,
            first_kept_stop: "A".into(),
            last_kept_stop: "B".into(),
            skipped_stops: vec!["C".into()],
            detour: None,
            added_secs: 300,
            start_date: chrono::NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap(),
            end_date: chrono::NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap(),
            extra: Default::default(),
        }),
        cancel: None,
        status,
        rev,
        updated_at,
        extra: Default::default(),
    }
}

fn synced(client: Vec<EditRecord>, server: Vec<EditRecord>, now_ms: u64) -> Vec<EditRecord> {
    match dispatch(Request::SyncEdits { client, server, now_ms }).unwrap() {
        Response::SyncEdits { records, .. } => records,
        _ => panic!("sync_edits"),
    }
}

#[test]
fn merges_lww_and_submits() {
    // Client arrives with a draft; the server already holds a newer
    // tombstone for one record and nothing for the other.
    let draft = record("e-new", EditStatus::Draft, 1, 1);
    let mut retired = record("e-old", EditStatus::Accepted, 3, 2);
    retired.status = EditStatus::Retired;

    let mut stale_local = retired.clone();
    stale_local.status = EditStatus::Accepted;
    stale_local.updated_at = 1;

    let records = synced(vec![draft, stale_local], vec![retired], 1_000);
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["e-new", "e-old"], "union of ids, sorted");
    assert_eq!(records[0].status, EditStatus::Submitted, "drafts submit");
    assert_eq!(records[0].rev, 2, "submit bumps rev");
    assert_eq!(records[0].updated_at, 1_000, "submit stamps the host clock");
    assert_eq!(records[1].status, EditStatus::Retired, "the tombstone (newer timestamp) won");
    assert_eq!(records[1].rev, 3, "past-draft records pass through untouched");
}

#[test]
fn the_pair_is_order_insensitive_and_deterministic() {
    // client and server are roles, not order: the same two documents
    // converge to the same answer either way — the semilattice the
    // library property-tests, pinned through the seam.
    let a = record("e1", EditStatus::Accepted, 2, 7);
    let mut b = record("e2", EditStatus::Draft, 1, 3);

    let one = synced(vec![a.clone(), b.clone()], vec![], 100);
    let two = synced(vec![], vec![b.clone(), a], 100);
    assert_eq!(one, two);
    assert_eq!(serde_json::to_string(&one).unwrap(), serde_json::to_string(&two).unwrap());

    // The submit transition is idempotent given the same clock: a
    // document already submitted does not bump again.
    b.status = EditStatus::Submitted;
    b.rev = 2;
    let once = synced(vec![b.clone()], vec![], 100);
    assert_eq!(once[0].rev, 2, "submitted records stay put");
}

#[test]
fn empty_documents_converge_to_empty() {
    // The server's first day: both sides empty, the answer is empty
    // — the protocol's zero case (the Go round-trip pins the same).
    assert!(synced(vec![], vec![], 0).is_empty());
}

#[test]
fn garbage_and_bulk_bytes_are_errors_not_panics() {
    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_json(r#"{"op":"sync_edits","client":"not a list","server":[],"now_ms":0}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_bytes(0, b"whatever");
    assert!(bad.contains("unknown bulk op"), "the edit document is JSON: {bad}");

    // The tagged wire form round-trips.
    let ok = dispatch_json(r#"{"op":"sync_edits","client":[],"server":[],"now_ms":0}"#);
    assert!(ok.contains(r#""records":[]"#), "{ok}");
}

#[test]
fn sync_edits_clamps_the_client_clock() {
    // D-56: a client whose clock runs fast cannot mint a record that
    // beats every honest write — the receive path clamps to now_ms.
    let future = record("e1", EditStatus::Accepted, 5, 10_000);
    let mut tombstone = record("e1", EditStatus::Accepted, 3, 2);
    tombstone.status = EditStatus::Retired;
    tombstone.updated_at = 6_000;

    let out = match dispatch(Request::SyncEdits {
        client: vec![future],
        server: vec![tombstone.clone()],
        now_ms: 5_000,
    })
    .unwrap()
    {
        Response::SyncEdits { records, .. } => records,
        _ => panic!("sync_edits"),
    };
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].status, EditStatus::Retired, "the clamped stamp (5000) loses to the honest 6000");
    assert_eq!(out[0].updated_at, 6_000);

    // Without the tombstone the clamped record stands at now_ms.
    let out = synced(vec![record("e1", EditStatus::Draft, 1, 10_000)], vec![], 5_000);
    assert_eq!(out[0].updated_at, 5_000, "submit re-stamps drafts at the host clock anyway");
}

#[test]
fn the_answer_carries_the_canonical_document_and_its_hash() {
    let client = vec![record("e1", EditStatus::Draft, 1, 1)];
    match dispatch(Request::SyncEdits { client, server: vec![], now_ms: 1_000 }).unwrap() {
        Response::SyncEdits { records, document, document_hash, .. } => {
            let doc = transit_sync::EditDocument::new(records.clone());
            assert_eq!(document, cap_abi::canonical_bytes(&doc).unwrap(), "canonical bytes: the tagged document");
            // canonical (RFC 8785, D-169): keys sorted, the tags among them
            assert!(document.starts_with("{\"kind\":\"edit_records\",\"records\":[") && document.ends_with("],\"schema_version\":2}"), "{document}");
            assert_eq!(document_hash, cap_abi::canonical_hash(&doc).unwrap(), "the module is the hash authority");
            assert!(document_hash.starts_with("sha256:"), "{document_hash}");
        }
        _ => panic!("sync_edits"),
    }
}

#[test]
fn read_edits_reads_a_received_document_at_receive() {
    let good = serde_json::to_value(record("e1", EditStatus::Submitted, 2, 100)).unwrap();
    let mut old = good.clone();
    let d = old["diversion"].as_object_mut().unwrap();
    for f in ["direction_id", "skipped_stops", "detour", "added_secs"] {
        d.remove(f);
    }
    let mut bad_date = good.clone();
    bad_date["diversion"]["start_date"] = "soon".into();
    let doc = serde_json::json!({"kind": "edit_records", "schema_version": 2, "records": [good, old, bad_date, {"id": "x"}, 7]});

    // A v1 document (the bare array) is the migration capability's.
    let v1 = doc["records"].clone();
    let refused = dispatch(Request::ReadEdits { document: v1 }).unwrap_err();
    assert!(refused.contains("edit_records v1") && refused.contains("migrate"), "{refused}");

    match dispatch(Request::ReadEdits { document: doc }).unwrap() {
        Response::ReadEdits { records, dropped, schema_version, document, document_hash } => {
            assert_eq!(records.len(), 2, "defaults fill what age removed");
            assert_eq!(dropped, 3, "malformed records are counted, never wedging the document");
            assert_eq!(records[1].diversion.as_ref().unwrap().added_secs, 0);
            assert_eq!(schema_version, transit_sync::SCHEMA_VERSION);
            let doc = transit_sync::EditDocument::new(records.clone());
            assert_eq!(document, cap_abi::canonical_bytes(&doc).unwrap());
            assert_eq!(document_hash, cap_abi::canonical_hash(&doc).unwrap());
        }
        _ => panic!("read_edits"),
    }
}

#[test]
fn compact_edits_is_the_d57_tombstone_commit() {
    let now = 1_000_000u64;
    let horizon = 100_000u64;
    let records = vec![
        record("old-retired", EditStatus::Retired, 2, now - 200_000),
        record("live", EditStatus::Accepted, 9, now - 900_000),
    ];
    match dispatch(Request::CompactEdits { records, now_ms: now, horizon_ms: horizon }).unwrap() {
        Response::CompactEdits { records, document, document_hash, dropped } => {
            assert_eq!(dropped, 1);
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].id, "live", "in-force records are never dropped");
            let doc = transit_sync::EditDocument::new(records.clone());
            assert_eq!(document, cap_abi::canonical_bytes(&doc).unwrap());
            assert_eq!(document_hash, cap_abi::canonical_hash(&doc).unwrap());
        }
        _ => panic!("compact_edits"),
    }
}

/// A field the client's record carries that this build does not know is
/// kept through the merge and flagged, never refused (D-167); the
/// server's own records are not flagged again.
#[test]
fn sync_edits_flags_the_clients_unknown_fields_and_keeps_them() {
    let mut client = serde_json::to_value(record("e1", EditStatus::Draft, 1, 10)).unwrap();
    client["diversion"]["flagger_needed"] = serde_json::json!(true);
    let mut server = serde_json::to_value(record("e2", EditStatus::Accepted, 1, 5)).unwrap();
    server["from_cad"] = serde_json::json!("CAD-7");
    let request = serde_json::json!({"op": "sync_edits", "client": [client], "server": [server], "now_ms": 1_000});
    let answer: serde_json::Value = serde_json::from_str(&dispatch_json(&request.to_string())).unwrap();
    let ok = &answer["ok"];
    assert_eq!(ok["unknown_fields"], serde_json::json!(["flagger_needed"]));
    let records = ok["records"].as_array().unwrap();
    assert_eq!(records[0]["diversion"]["flagger_needed"], true);
    assert_eq!(records[1]["from_cad"], "CAD-7");
}

// ---------------------------------------------------------------------
// The disruptions log's records (D-190, D-197). The log capability keeps
// the log; here transit-log stands in for it, as cap-log calls it.
// ---------------------------------------------------------------------

use std::collections::BTreeMap;
use transit_log::{Checkpoint, Composed, Header, Held, Intent, KindRecords};

fn hash(v: &serde_json::Value) -> String {
    cap_abi::canonical_hash(v).unwrap()
}

fn header(seq: u64, at_ms: u64) -> Header {
    Header {
        log: "disruptions".into(),
        seq,
        writer: "dispatcher@example".into(),
        at_ms,
        module: "cap-log@sha256:test".into(),
        intent: Intent { op: "POST /edits".into(), ..Default::default() },
    }
}

fn log_records(document: transit_sync::EditDocument) -> Vec<KindRecords> {
    match dispatch(Request::LogRecords { document }).unwrap() {
        Response::LogRecords { next } => next,
        _ => panic!("log_records"),
    }
}

fn log_document(fold: &Checkpoint) -> (String, String) {
    let records: BTreeMap<String, Held> = fold.records.get("edit_record").cloned().unwrap_or_default();
    let document = fold.get("edit_records", transit_log::DOCUMENT_ID).cloned();
    match dispatch(Request::LogDocument { records, document }).unwrap() {
        Response::LogDocument { document, document_hash } => (document, document_hash),
        _ => panic!("log_document"),
    }
}

/// One write as the server runs it: merge, the records, and the entry
/// applied to the fold. Answers the merged records and whether the
/// write changed a record.
fn write(fold: &mut Checkpoint, client: Vec<EditRecord>, now_ms: u64) -> (Vec<EditRecord>, Option<transit_log::Entry>) {
    let (document, _) = log_document(fold);
    let head: transit_sync::EditDocument = serde_json::from_str(&document).unwrap();
    let next = synced(client, head.records, now_ms);
    let records = log_records(transit_sync::EditDocument::new(next.clone()));
    match transit_log::compose(fold, header(fold.seq + 1, now_ms), &records, &hash) {
        Composed::Entry(e) => {
            transit_log::apply(fold, &e, &hash).unwrap();
            (next, Some(e))
        }
        _ => (next, None),
    }
}

/// Three writes, and the fold and entries they leave.
fn three_writes() -> (Vec<EditRecord>, Checkpoint, Vec<transit_log::Entry>) {
    let mut fold = Checkpoint::empty("disruptions");
    let mut entries = Vec::new();
    let mut head = Vec::new();
    for (i, client) in [
        vec![record("e1", EditStatus::Draft, 1, 10)],
        vec![record("e2", EditStatus::Draft, 1, 20)],
        vec![record("e1", EditStatus::Retired, 3, 250)],
    ]
    .into_iter()
    .enumerate()
    {
        let (next, entry) = write(&mut fold, client, 100 * (i as u64 + 1));
        entries.extend(entry);
        head = next;
    }
    (head, fold, entries)
}

#[test]
fn the_folded_records_assemble_the_merged_document() {
    let (head, fold, entries) = three_writes();
    assert_eq!(entries.len(), 3);
    let (document, document_hash) = log_document(&fold);
    let whole = transit_sync::EditDocument::new(head);
    assert_eq!(document, cap_abi::canonical_bytes(&whole).unwrap(), "byte for byte");
    assert_eq!(document_hash, cap_abi::canonical_hash(&whole).unwrap());
    let kinds: Vec<&str> = entries[0].changes.iter().map(|c| c.kind.as_str()).collect();
    assert_eq!(kinds, vec!["edit_record", "edit_records"], "the record, and the document's own record");
    assert_eq!(entries[1].changes.len(), 1, "a later write changes only its record");
    assert_eq!(entries[1].changes[0].before, None);
}

#[test]
fn a_write_that_changes_nothing_changes_no_record() {
    let (head, mut fold, _) = three_writes();
    let (_, entry) = write(&mut fold, head, 1_000);
    assert!(entry.is_none(), "a resend is idempotent: no entry");
}

#[test]
fn the_documents_own_fields_survive_its_records() {
    let mut doc = transit_sync::EditDocument::new(vec![record("e1", EditStatus::Accepted, 2, 10)]);
    doc.extra.0.insert("issued_by".into(), serde_json::json!("dispatch desk"));
    let mut fold = Checkpoint::empty("disruptions");
    let Composed::Entry(e) = transit_log::compose(&fold, header(1, 1), &log_records(doc.clone()), &hash) else { panic!("an entry") };
    transit_log::apply(&mut fold, &e, &hash).unwrap();
    assert_eq!(log_document(&fold).0, cap_abi::canonical_bytes(&doc).unwrap());
}

#[test]
fn a_record_in_another_version_is_refused() {
    let records = BTreeMap::from([("e1".to_string(), Held { version: 3, value: serde_json::to_value(record("e1", EditStatus::Draft, 1, 1)).unwrap() })]);
    let refused = dispatch(Request::LogDocument { records, document: None }).unwrap_err();
    assert!(refused.contains("e1") && refused.contains('3') && refused.contains('2'), "{refused}");
}

#[test]
fn a_revert_stamps_each_record_to_win_the_merge_and_retires_what_it_created() {
    let (head, mut fold, entries) = three_writes();
    // Revert entry 3 (e1 retired) and entry 2 (e2 created), as the
    // server does: cap-log's states, then log_revert's records.
    let mut before = Checkpoint::empty("disruptions");
    transit_log::apply(&mut before, &entries[0], &hash).unwrap();
    let mid = {
        let mut f = before.clone();
        transit_log::apply(&mut f, &entries[1], &hash).unwrap();
        f
    };
    let states = transit_log::revert_states(&mid, &fold, &entries[2], &hash).unwrap();
    let Response::LogRevert { written } = dispatch(Request::LogRevert { states: states.clone(), now_ms: 5_000 }).unwrap() else { panic!("log_revert") };
    assert_eq!(written.len(), 1, "the edit record, not the document's own record");
    let changes = transit_log::revert_with(&states, &written, &hash).unwrap();
    transit_log::apply(&mut fold, &transit_log::Entry::new(header(4, 5_000), changes, Some(3)), &hash).unwrap();
    let (document, _) = log_document(&fold);
    let doc: transit_sync::EditDocument = serde_json::from_str(&document).unwrap();
    let e1 = doc.records.iter().find(|r| r.id == "e1").unwrap();
    assert_eq!((e1.status, e1.updated_at, e1.rev), (EditStatus::Submitted, 5_000, 4), "the state before entry 3, stamped");
    // A page that still holds the retired copy does not undo the revert.
    assert_eq!(synced(head, doc.records.clone(), 6_000), doc.records);

    // Entry 2 created e2: its revert retires e2, and keeps it in the
    // document, because the merge would bring a dropped record back.
    let states = transit_log::revert_states(&before, &mid, &entries[1], &hash).unwrap();
    let Response::LogRevert { written } = dispatch(Request::LogRevert { states, now_ms: 7_000 }).unwrap() else { panic!("log_revert") };
    let e2: EditRecord = serde_json::from_value(written[0].after.clone().unwrap()).unwrap();
    assert_eq!((e2.id.as_str(), e2.status, e2.updated_at), ("e2", EditStatus::Retired, 7_000));
}

// ---------------------------------------------------------------------
// receive: the disruptions log's write, whole (D-228)
// ---------------------------------------------------------------------

fn receive_header(seq: u64) -> serde_json::Value {
    serde_json::json!({ "log": "disruptions", "seq": seq, "writer": "dispatch-1", "at_ms": 5_000, "module": "cap-sync@test", "intent": { "op": "POST /edits" } })
}

fn ok(req: serde_json::Value) -> serde_json::Value {
    let out: serde_json::Value = serde_json::from_str(&dispatch_json(&req.to_string())).unwrap();
    out.get("ok").cloned().unwrap_or_else(|| panic!("{req}: {out}"))
}

#[test]
fn receive_merges_a_client_document_and_answers_its_entry() {
    // A bare array is an edit document v1: it comes forward in the op.
    let client = serde_json::to_value(vec![record("e1", EditStatus::Draft, 1, 9_000)]).unwrap();
    let got = ok(serde_json::json!({ "op": "receive", "write": { "header": receive_header(1) },
        "ops": [{ "cap": "sync", "op": "sync", "args": { "client": client } }] }));
    let entry: serde_json::Value = serde_json::from_str(got["entry"].as_str().unwrap_or_else(|| panic!("an entry: {got}"))).unwrap();
    let change = entry["changes"].as_array().unwrap().iter().find(|c| c["kind"] == "edit_record").expect("the record changes");
    assert_eq!(change["after"]["status"], "submitted", "drafts are submitted: {change}");
    assert_eq!(change["after"]["updated_at"], 5_000, "the client's clock is clamped to the entry's (D-56): {change}");
    assert!(got["heads"]["edit_records"].is_string(), "{got}");
}

#[test]
fn receive_answers_a_moved_head_and_compacts_over_the_head() {
    let doc = transit_sync::EditDocument::new(vec![record("e1", EditStatus::Retired, 2, 1_000)]);
    let docv = serde_json::to_value(&doc).unwrap();
    let moved = ok(serde_json::json!({ "op": "receive", "documents": { "edit_records": docv }, "write": { "header": receive_header(1) },
        "ops": [{ "cap": "sync", "op": "sync", "args": { "client": [], "from": "sha256:old" } }] }));
    assert!(moved["moved"]["current"].as_str().unwrap().starts_with("sha256:"), "{moved}");
    // The fold holds the tombstone (entry 1), then the compaction drops it.
    let first = ok(serde_json::json!({ "op": "receive", "documents": { "edit_records": docv }, "write": { "header": receive_header(1) },
        "ops": [{ "cap": "sync", "op": "migrate" }] }));
    let stored = [transit_log::Stored { seq: 1, text: first["entry"].as_str().unwrap().into() }];
    let fold = transit_log::fold(Checkpoint::empty("disruptions"), &stored, &cap_abi::record_hash).unwrap();
    let compacted = ok(serde_json::json!({ "op": "receive", "documents": { "edit_records": docv },
        "write": { "checkpoint": serde_json::to_value(fold.fold).unwrap(), "header": receive_header(2) },
        "ops": [{ "cap": "sync", "op": "compact", "args": { "horizon_ms": 1_000 } }] }));
    let entry: serde_json::Value = serde_json::from_str(compacted["entry"].as_str().unwrap_or_else(|| panic!("{compacted}"))).unwrap();
    assert!(entry["changes"].as_array().unwrap().iter().any(|c| c["kind"] == "edit_record" && c["after"].is_null()), "the old tombstone leaves the document: {entry}");
}

/// D-231: compaction skips an old tombstone that a held alert names, and
/// still drops one that nothing names.
#[test]
fn receive_compact_keeps_a_tombstone_that_a_held_record_names() {
    let doc = transit_sync::EditDocument::new(vec![record("e1", EditStatus::Retired, 2, 1_000), record("e2", EditStatus::Retired, 2, 1_000)]);
    let docv = serde_json::to_value(&doc).unwrap();
    let first = ok(serde_json::json!({ "op": "receive", "documents": { "edit_records": docv }, "write": { "header": receive_header(1) },
        "ops": [{ "cap": "sync", "op": "migrate" }] }));
    let stored = [transit_log::Stored { seq: 1, text: first["entry"].as_str().unwrap().into() }];
    let fold = transit_log::fold(Checkpoint::empty("disruptions"), &stored, &cap_abi::record_hash).unwrap();
    let kinds = serde_json::json!([{ "store_kind": "service_alerts", "log": "alerts",
        "records": [{ "kind": "service_alert", "collection": "alerts" }],
        "references": [{ "path": "/alerts/*/explains/*[kind=edit]/id", "target": "edit_record" }] }]);
    let alerts = serde_json::json!({ "service_alert": { "a1": { "version": 1, "value": { "status": "retired", "explains": [{ "kind": "edit", "id": "e1" }] } } } });
    let compacted = ok(serde_json::json!({ "op": "receive", "documents": { "edit_records": docv },
        "write": { "checkpoint": serde_json::to_value(fold.fold).unwrap(), "header": receive_header(2),
                   "references": kinds, "now": { "alerts": alerts } },
        "ops": [{ "cap": "sync", "op": "compact", "args": { "horizon_ms": 1_000 } }] }));
    let entry: serde_json::Value = serde_json::from_str(compacted["entry"].as_str().unwrap_or_else(|| panic!("{compacted}"))).unwrap();
    let dropped: Vec<&str> = entry["changes"].as_array().unwrap().iter().filter(|c| c["kind"] == "edit_record" && c["after"].is_null()).map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(dropped, vec!["e2"], "a1 names e1, so e1 stays: {entry}");
}

#[test]
fn receive_migrate_records_the_head_and_another_log_s_op_fails() {
    let doc = serde_json::to_value(vec![record("e1", EditStatus::Submitted, 1, 1_000)]).unwrap();
    let got = ok(serde_json::json!({ "op": "receive", "documents": { "edit_records": doc }, "write": { "header": receive_header(1) },
        "ops": [{ "cap": "sync", "op": "migrate" }] }));
    assert!(got["entry"].is_string(), "{got}");
    let other = ok(serde_json::json!({ "op": "receive", "write": { "header": receive_header(1) }, "ops": [{ "cap": "stops", "op": "add_event" }] }));
    assert!(other["failed"]["error"].as_str().unwrap().contains("not a write to the disruptions log"), "{other}");
}
