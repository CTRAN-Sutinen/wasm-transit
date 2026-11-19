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
//! 7. the disruptions log (D-190): the fold of the entries is the merged
//!    document, a write that changes nothing answers no entry, a bad
//!    entry is quarantined and named, a revert writes back the states
//!    before and is refused after a later change, and a gap closes
//!    only with a cause.

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
// The disruptions log (D-190): the entry, the fold, the revert, the gap
// ---------------------------------------------------------------------

use transit_sync::log::{Header, Intent, Stored};

fn header(seq: u64, at_ms: u64) -> Header {
    Header {
        seq,
        writer: "dispatcher@example".into(),
        at_ms,
        module: "cap-sync@sha256:test".into(),
        intent: Intent { op: "POST /edits".into(), ..Default::default() },
    }
}

/// One write through the seam as the server runs it: merge, then the
/// entry from the head and the next document.
fn write(head: &[EditRecord], client: Vec<EditRecord>, seq: u64, now_ms: u64) -> (Vec<EditRecord>, Option<String>) {
    let next = synced(client, head.to_vec(), now_ms);
    let (h, n) = (transit_sync::EditDocument::new(head.to_vec()), transit_sync::EditDocument::new(next.clone()));
    match dispatch(Request::EntryDisruptions { head: h, next: n, header: header(seq, now_ms) }).unwrap() {
        Response::EntryDisruptions { entry, .. } => (next, entry),
        _ => panic!("entry_disruptions"),
    }
}

fn fold(checkpoint: Option<serde_json::Value>, entries: Vec<Stored>) -> (u64, String, String, String, usize) {
    match dispatch(Request::FoldDisruptions { checkpoint, entries }).unwrap() {
        Response::FoldDisruptions { seq, document, document_hash, checkpoint, quarantined } => (seq, document, document_hash, checkpoint, quarantined.len()),
        _ => panic!("fold_disruptions"),
    }
}

/// Three writes, and the log they leave.
fn three_writes() -> (Vec<EditRecord>, Vec<Stored>) {
    let mut head = Vec::new();
    let mut log = Vec::new();
    let writes = vec![
        vec![record("e1", EditStatus::Draft, 1, 10)],
        vec![record("e2", EditStatus::Draft, 1, 20)],
        vec![{
            let mut r = record("e1", EditStatus::Retired, 3, 250);
            r.status = EditStatus::Retired;
            r
        }],
    ];
    for (i, client) in writes.into_iter().enumerate() {
        let (next, entry) = write(&head, client, log.len() as u64 + 1, 100 * (i as u64 + 1));
        if let Some(text) = entry {
            log.push(Stored { seq: log.len() as u64 + 1, text });
        }
        head = next;
    }
    (head, log)
}

#[test]
fn the_fold_of_the_entries_is_the_merged_document() {
    let (head, log) = three_writes();
    assert_eq!(log.len(), 3);
    let (seq, document, hash, _, quarantined) = fold(None, log.clone());
    let whole = transit_sync::EditDocument::new(head);
    assert_eq!((seq, quarantined), (3, 0));
    assert_eq!(document, cap_abi::canonical_bytes(&whole).unwrap());
    assert_eq!(hash, cap_abi::canonical_hash(&whole).unwrap());

    // A fold from the checkpoint at entry 2 reaches the same document.
    let (_, _, _, at2, _) = fold(None, log[..2].to_vec());
    let (seq, _, again, checkpoint, _) = fold(Some(serde_json::from_str(&at2).unwrap()), log[2..].to_vec());
    assert_eq!((seq, again), (3, hash));
    assert!(checkpoint.contains("\"kind\":\"disruptions_checkpoint\"") && checkpoint.contains("\"seq\":3"), "{checkpoint}");
}

#[test]
fn a_write_that_changes_nothing_answers_no_entry() {
    let (head, _) = three_writes();
    let (_, entry) = write(&head, head.clone(), 4, 1_000);
    assert_eq!(entry, None, "a resend is idempotent: no entry");
}

#[test]
fn an_entry_holds_each_changed_record_with_its_hash_before() {
    let before = vec![record("e1", EditStatus::Submitted, 2, 10)];
    let (_, entry) = write(&before, vec![record("e1", EditStatus::Accepted, 3, 50)], 7, 100);
    let entry: serde_json::Value = serde_json::from_str(&entry.unwrap()).unwrap();
    assert_eq!(entry["kind"], "disruptions_entry");
    assert_eq!(entry["log"], "disruptions");
    assert_eq!(entry["seq"], 7);
    assert_eq!(entry["writer"], "dispatcher@example");
    assert_eq!(entry["module"], "cap-sync@sha256:test");
    assert_eq!(entry["intent"]["op"], "POST /edits");
    let change = &entry["changes"][0];
    assert_eq!(change["id"], "e1");
    assert_eq!(change["before"], cap_abi::canonical_hash(&before[0]).unwrap());
    assert_eq!(change["after"]["status"], "accepted");
}

#[test]
fn a_bad_entry_is_quarantined_and_named() {
    let (_, mut log) = three_writes();
    log[1].text = "{\"kind\":\"disruptions_entry\"".into();
    match dispatch(Request::FoldDisruptions { checkpoint: None, entries: log }).unwrap() {
        Response::FoldDisruptions { seq, quarantined, .. } => {
            assert_eq!(seq, 3);
            // Entry 2 does not read, and entry 3 then still applies: it
            // changes e1, which entry 1 wrote.
            assert_eq!(quarantined.iter().map(|q| q.seq).collect::<Vec<_>>(), vec![2]);
        }
        _ => panic!("fold_disruptions"),
    }
}

#[test]
fn a_revert_writes_back_the_states_before_and_is_refused_after_a_later_change() {
    let (head, log) = three_writes();
    // Revert entry 3 (e1 retired): e1 is in force again.
    let answer = dispatch(Request::RevertDisruptions { checkpoint: None, entries: log.clone(), target: 3, header: header(4, 5_000) }).unwrap();
    let Response::RevertDisruptions { entry, changes } = answer else { panic!("revert_disruptions") };
    assert_eq!(changes, 1);
    let mut after = log.clone();
    after.push(Stored { seq: 4, text: entry.clone() });
    let (_, document, _, _, quarantined) = fold(None, after.clone());
    assert_eq!(quarantined, 0);
    let doc: serde_json::Value = serde_json::from_str(&document).unwrap();
    assert_eq!(doc["records"][0]["id"], "e1");
    assert_eq!(doc["records"][0]["status"], "submitted", "the state before entry 3");
    assert_eq!(doc["records"][0]["updated_at"], 5_000, "stamped to win the merge");
    let entry: serde_json::Value = serde_json::from_str(&entry).unwrap();
    assert_eq!(entry["reverts"], 3);

    // A page that still holds the retired copy does not undo it.
    let reverted: Vec<EditRecord> = serde_json::from_value(doc["records"].clone()).unwrap();
    assert_eq!(synced(head, reverted.clone(), 6_000), reverted);

    // Entry 3 again: entry 4 changed e1 since, so a person decides.
    let refused = dispatch(Request::RevertDisruptions { checkpoint: None, entries: after, target: 3, header: header(5, 7_000) }).unwrap_err();
    assert!(refused.contains("record e1") && refused.contains("a person decides"), "{refused}");
}

#[test]
fn a_gap_closes_with_an_entry_that_names_its_cause() {
    let refused = dispatch(Request::CloseDisruptionsGap { header: header(9, 1) }).unwrap_err();
    assert!(refused.contains("cause"), "{refused}");
    let mut h = header(9, 1);
    h.intent = Intent { op: "close_disruptions_gap".into(), note: Some("entry 9 was deleted by hand".into()), ..Default::default() };
    let Response::CloseDisruptionsGap { entry } = dispatch(Request::CloseDisruptionsGap { header: h }).unwrap() else { panic!("close_disruptions_gap") };
    let entry: serde_json::Value = serde_json::from_str(&entry).unwrap();
    assert_eq!(entry["changes"], serde_json::json!([]));
    assert_eq!(entry["intent"]["note"], "entry 9 was deleted by hand");
}
