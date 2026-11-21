//! The existence check's laws (D-232, D-233): a declared reference names
//! a record its log's fold holds, in any state; a target the check
//! cannot see is not judged; and every record kind the kind table
//! names as a target has a log, so the check can judge it.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use transit_log::{blame, broken, values_at, Broken, EntryRef, Held, KindReferences, Records, Stored};

fn kinds() -> Vec<KindReferences> {
    serde_json::from_value(json!([
        {"store_kind": "stops", "log": "stops", "records": [{"kind": "stop", "collection": "stops"}],
         "references": [{"path": "/stops/*/*/replaces/*", "target": "stop"}]},
        {"store_kind": "stop_requests", "log": "stops", "records": [{"kind": "stop_request", "collection": "requests"}],
         "references": [{"path": "/requests/*/stop", "target": "stop"}, {"path": "/requests/*/routes/*", "target": "network.route"}]},
        {"store_kind": "service_alerts", "log": "alerts", "records": [{"kind": "service_alert", "collection": "alerts"}], "references": []},
        {"store_kind": "notes", "log": "notes", "records": [{"kind": "note", "collection": "notes"}],
         "references": [{"path": "/notes/*/target[kind=alert]/id", "target": "service_alert"},
                        {"path": "/notes/*/target[kind=stop]/stop", "target": "stop"}]}
    ]))
    .unwrap()
}

fn records(kinds: &[(&str, Value)]) -> Records {
    let mut out = Records::new();
    for (kind, by_id) in kinds {
        let held = by_id.as_object().unwrap().iter().map(|(id, v)| (id.clone(), Held { version: 1, value: v.clone() })).collect();
        out.insert(kind.to_string(), held);
    }
    out
}

fn stops_fold() -> Records {
    records(&[
        ("stop", json!({
            "A": [{"on": "2020-01-01", "event": "in_service"}],
            "B": [{"on": "2020-01-01", "event": "in_service"}, {"on": "2026-01-01", "event": "retired"}],
            "C": [{"on": "2026-02-01", "event": "planned", "replaces": ["B", "Z"]}]
        })),
        ("stop_request", json!({
            "R-1": {"stop": "A", "routes": ["99"]},
            "R-2": {"stop": "B"},
            "R-3": {"stop": "GONE"}
        })),
    ])
}

fn broken_of(folds: &[(&str, Records)]) -> Vec<Broken> {
    let folds: BTreeMap<String, Records> = folds.iter().map(|(l, r)| (l.to_string(), r.clone())).collect();
    broken(&kinds(), &folds)
}

fn b(log: &str, kind: &str, id: &str, field: &str, target: &str, names: &str) -> Broken {
    Broken { log: log.into(), kind: kind.into(), id: id.into(), field: field.into(), target: target.into(), names: names.into(), entry: None }
}

#[test]
fn a_reference_to_a_record_the_fold_does_not_hold_is_broken_and_a_retired_one_is_not() {
    assert_eq!(
        broken_of(&[("stops", stops_fold())]),
        vec![
            b("stops", "stop", "C", "/0/replaces/1", "stop", "Z"),
            b("stops", "stop_request", "R-3", "/stop", "stop", "GONE"),
        ],
        "R-2 names B, which is retired and still held; the network's routes are not judged"
    );
}

#[test]
fn a_reference_into_another_log_is_judged_only_when_that_fold_is_given() {
    let notes = records(&[("note", json!({
        "n1": {"target": {"kind": "alert", "id": "a1"}},
        "n2": {"target": {"kind": "alert", "id": "a9"}},
        "n3": {"target": {"kind": "stop", "stop": "GONE"}},
        "n4": {"target": {"kind": "edit", "id": "a9"}}
    }))]);
    let alerts = records(&[("service_alert", json!({"a1": {}}))]);
    assert_eq!(broken_of(&[("notes", notes.clone())]), vec![], "neither the alerts log nor the stops log is given");
    assert_eq!(
        broken_of(&[("notes", notes.clone()), ("alerts", alerts.clone())]),
        vec![b("notes", "note", "n2", "/target/id", "service_alert", "a9")],
        "the filter reads only an alert target; n4 names an edit"
    );
    assert_eq!(
        broken_of(&[("notes", notes), ("alerts", alerts), ("stops", stops_fold())]),
        vec![
            b("notes", "note", "n2", "/target/id", "service_alert", "a9"),
            b("notes", "note", "n3", "/target/stop", "stop", "GONE"),
            b("stops", "stop", "C", "/0/replaces/1", "stop", "Z"),
            b("stops", "stop_request", "R-3", "/stop", "stop", "GONE"),
        ]
    );
}

#[test]
fn a_path_reads_fields_each_element_and_filters_with_pointers() {
    let v = json!({"a": [{"k": "x", "id": "1"}, {"k": "y", "id": "2"}], "m": {"p/q": {"id": "3"}}});
    let path = |p: &str| p.split('/').map(str::to_string).collect::<Vec<_>>();
    let at = |p: &str| values_at(&v, &path(p)).into_iter().map(|(f, v)| (f, v.clone())).collect::<Vec<_>>();
    assert_eq!(at("a/*/id"), vec![("/a/0/id".into(), json!("1")), ("/a/1/id".into(), json!("2"))]);
    assert_eq!(at("a/*[k=y]/id"), vec![("/a/1/id".into(), json!("2"))]);
    assert_eq!(at("m/*/id"), vec![("/m/p~1q/id".into(), json!("3"))], "a pointer escapes its tokens");
    assert_eq!(at("a/id"), vec![], "a field of an array is nothing");
}

/// The kind table as `just caps` writes it: each target that is a record
/// kind is kept by a log, so the check can judge every such reference.
#[test]
fn every_record_kind_a_kind_names_has_a_log() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../cap-abi/kinds.json")).unwrap();
    let table: Value = serde_json::from_str(&text).unwrap();
    let kinds: Vec<KindReferences> = serde_json::from_value(table["kinds"].clone()).unwrap();
    let logged: Vec<&str> = kinds.iter().filter(|k| k.log.is_some()).flat_map(|k| k.records.iter().map(|r| r.kind.as_str())).collect();
    let mut judged = 0;
    for k in &kinds {
        for d in &k.references {
            let record_kind = !d.target.contains('.') && !["attachment", "trip_instance", "network", "streets", "feed"].contains(&d.target.as_str());
            if record_kind {
                assert!(logged.contains(&d.target.as_str()), "{} names {} at {}, which no log keeps", k.store_kind, d.target, d.path);
                judged += 1;
            }
        }
    }
    assert!(judged >= 6, "the table's record references: {judged}");
}

/// An entry as a log stores it: its seq, its clock, and the records it
/// changes (`after` absent: the record leaves the fold).
fn entry(log: &str, seq: u64, at_ms: u64, changes: &[(&str, &str, bool)]) -> Stored {
    let changes: Vec<Value> = changes
        .iter()
        .map(|(kind, id, after)| if *after { json!({"kind": kind, "version": 1, "id": id, "after": {}}) } else { json!({"kind": kind, "version": 1, "id": id, "before": "sha256:x"}) })
        .collect();
    let text = json!({"kind": "log_entry", "schema_version": 1, "log": log, "seq": seq, "writer": "w", "at_ms": at_ms, "module": "m", "intent": {"op": "test"}, "changes": changes});
    Stored { seq, text: text.to_string() }
}

/// The entry that broke a reference is the later of the referrer's last
/// write and the target's last change: by seq in one log, by clock
/// across two. A target never written names the referrer's entry.
#[test]
fn blame_names_the_later_of_the_referrers_write_and_the_targets_change() {
    let mut found = vec![
        b("stops", "stop_request", "R-3", "/stop", "stop", "GONE"),
        b("stops", "stop_request", "R-4", "/stop", "stop", "B"),
        b("notes", "note", "n2", "/target/id", "service_alert", "a9"),
    ];
    let entries: BTreeMap<String, Vec<Stored>> = [
        ("stops".to_string(), vec![
            entry("stops", 1, 100, &[("stop", "B", true), ("stop_request", "R-3", true)]),
            entry("stops", 2, 200, &[("stop_request", "R-4", true)]),
            entry("stops", 3, 300, &[("stop", "B", false)]),
        ]),
        ("notes".to_string(), vec![entry("notes", 1, 500, &[("note", "n2", true)])]),
        ("alerts".to_string(), vec![entry("alerts", 7, 900, &[("service_alert", "a9", true)]), entry("alerts", 8, 950, &[("service_alert", "a9", false)])]),
    ]
    .into_iter()
    .collect();
    blame(&mut found, &kinds(), &entries);
    let named: Vec<Option<EntryRef>> = found.iter().map(|b| b.entry.clone()).collect();
    assert_eq!(
        named,
        vec![
            Some(EntryRef { log: "stops".into(), seq: 1 }),
            Some(EntryRef { log: "stops".into(), seq: 3 }),
            Some(EntryRef { log: "alerts".into(), seq: 8 }),
        ],
        "GONE was never written: R-3's own entry; B left the fold after R-4 named it; a9 left the alerts log after the note"
    );
}

/// A write answers the broken references it touches: one it wrote
/// broken, and one whose target it took out. The others are not its.
#[test]
fn an_entry_touches_the_references_it_wrote_and_the_targets_it_changed() {
    let found = vec![
        b("stops", "stop_request", "R-3", "/stop", "stop", "GONE"),
        b("stops", "stop_request", "R-4", "/stop", "stop", "B"),
        b("notes", "note", "n2", "/target/id", "service_alert", "a9"),
    ];
    let wrote = transit_log::touched(found.clone(), &kinds(), "stops", &entry("stops", 5, 1, &[("stop_request", "R-3", true)]));
    assert_eq!(wrote.iter().map(|b| (b.id.as_str(), b.entry.clone())).collect::<Vec<_>>(), vec![("R-3", Some(EntryRef { log: "stops".into(), seq: 5 }))]);
    let removed = transit_log::touched(found.clone(), &kinds(), "stops", &entry("stops", 6, 1, &[("stop", "B", false)]));
    assert_eq!(removed.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), vec!["R-4"]);
    let other = transit_log::touched(found, &kinds(), "alerts", &entry("alerts", 2, 1, &[("service_alert", "a9", false)]));
    assert_eq!(other.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), vec!["n2"], "a target in another log");
}

/// After a rollback a fold can hold a record in a version newer than the
/// table reads (D-186). Its references are not judged: it is listed as
/// unjudged, a warning, and never as broken.
#[test]
fn a_record_newer_than_the_table_reads_is_unjudged_and_never_broken() {
    let mut kinds = kinds();
    for k in &mut kinds {
        k.versions = vec![transit_log::KindVersion { version: 1 }];
    }
    let mut fold = stops_fold();
    fold.get_mut("stop_request").unwrap().get_mut("R-3").unwrap().version = 2;
    let folds: BTreeMap<String, Records> = [("stops".to_string(), fold)].into_iter().collect();
    assert_eq!(broken(&kinds, &folds), vec![b("stops", "stop", "C", "/0/replaces/1", "stop", "Z")], "R-3 is not judged");
    assert_eq!(
        transit_log::unjudged(&kinds, &folds),
        vec![transit_log::Unjudged { log: "stops".into(), kind: "stop_request".into(), id: "R-3".into(), version: 2, reads: 1 }]
    );
}

/// What a pointer's revert drops is named by hash: every declared
/// reference whose value is a dropped hash, whatever its target.
#[test]
fn naming_finds_each_reference_to_a_dropped_hash() {
    let kinds: Vec<KindReferences> = serde_json::from_value(json!([
        {"store_kind": "frozen_versions", "log": "plans", "records": [{"kind": "frozen_version", "collection": "versions"}],
         "references": [{"path": "/versions/*/network_hash", "target": "network"}]}
    ]))
    .unwrap();
    let folds: BTreeMap<String, Records> =
        [("plans".to_string(), records(&[("frozen_version", json!({"fall": {"network_hash": "sha256:aa"}, "spring": {"network_hash": "sha256:bb"}}))]))].into_iter().collect();
    let found = transit_log::naming(&kinds, &folds, &["sha256:bb".to_string(), "sha256:cc".to_string()]);
    assert_eq!(
        found,
        vec![transit_log::Naming { log: "plans".into(), kind: "frozen_version".into(), id: "spring".into(), field: "/network_hash".into(), target: "network".into(), names: "sha256:bb".into() }]
    );
}

/// The refusal as of the read seq (D-233): a write that adds a reference
/// to a record missing from what its writer read is refused; one the
/// writer saw, one the write itself creates, and one the record already
/// had are not; a target the write takes out is missing; a target log
/// the writer read nothing of is not judged.
#[test]
fn a_write_that_adds_a_reference_the_writer_could_not_have_seen_is_refused() {
    let read: BTreeMap<String, Records> = [("stops".to_string(), records(&[
        ("stop", json!({"A": [], "B": []})),
        ("stop_request", json!({"OLD": {"stop": "GONE"}})),
    ]))]
    .into_iter()
    .collect();
    let before = read["stops"].clone();
    let change = |kind: &str, id: &str, after: Option<Value>| match after {
        Some(a) => json!({"kind": kind, "version": 1, "id": id, "after": a}),
        None => json!({"kind": kind, "version": 1, "id": id, "before": "sha256:x"}),
    };
    let write = |changes: Vec<Value>| {
        let text = json!({"kind": "log_entry", "schema_version": 1, "log": "stops", "seq": 9, "writer": "w", "at_ms": 1, "module": "m", "intent": {"op": "test"}, "changes": changes});
        transit_log::added(&kinds(), &read, &before, "stops", &Stored { seq: 9, text: text.to_string() })
    };
    let ids = |found: Vec<Broken>| found.into_iter().map(|b| (b.id, b.names)).collect::<Vec<_>>();
    assert_eq!(ids(write(vec![change("stop_request", "R-1", Some(json!({"stop": "A"})))])), vec![], "the writer saw A");
    assert_eq!(ids(write(vec![change("stop_request", "R-2", Some(json!({"stop": "Z"})))])), vec![("R-2".to_string(), "Z".to_string())]);
    assert_eq!(
        ids(write(vec![change("stop", "N", Some(json!([]))), change("stop_request", "R-3", Some(json!({"stop": "N"})))])),
        vec![],
        "the write creates N"
    );
    assert_eq!(ids(write(vec![change("stop", "B", None), change("stop_request", "R-4", Some(json!({"stop": "B"})))])), vec![("R-4".to_string(), "B".to_string())], "the write takes B out");
    assert_eq!(ids(write(vec![change("stop_request", "OLD", Some(json!({"stop": "GONE", "status": "cancelled"})))])), vec![], "OLD was broken before: it can still be cancelled");
    let note = json!({"kind": "log_entry", "schema_version": 1, "log": "notes", "seq": 1, "writer": "w", "at_ms": 1, "module": "m", "intent": {"op": "test"},
        "changes": [{"kind": "note", "version": 1, "id": "n1", "after": {"target": {"kind": "alert", "id": "a9"}}}]});
    assert_eq!(transit_log::added(&kinds(), &read, &Records::new(), "notes", &Stored { seq: 1, text: note.to_string() }), vec![], "the writer read nothing of the alerts log");
}

/// The receive op judges a write's references (D-233): given the kind
/// table and each log as of the writer's read seqs, a write that adds a
/// reference to a record missing then is refused with `broken` and no
/// entry; one that held then and is broken now lands, with the reference
/// as its warning in `references`. A log not given is not judged.
#[test]
fn the_receive_op_refuses_as_of_the_read_and_warns_as_of_now() {
    let header = |seq: u64| serde_json::from_value::<transit_log::Header>(json!({"log": "stops", "seq": seq, "writer": "w", "at_ms": 1, "module": "m", "intent": {"op": "test"}})).unwrap();
    let request = |id: &str, stop: &str| transit_log::KindRecords { kind: "stop_request".into(), version: 1, records: [(id.to_string(), json!({"stop": stop}))].into_iter().collect() };
    let stops = |ids: &[&str]| records(&[("stop", Value::Object(ids.iter().map(|i| (i.to_string(), json!([]))).collect()))]);
    let canonical = |e: &transit_log::Entry| Ok(serde_json::to_string(e).unwrap());
    let hash = |v: &Value| format!("{v}");
    let write = |read: Records, now: Option<Records>| transit_log::Write {
        checkpoint: None,
        header: header(1),
        unread: vec![],
        references: Some(kinds()),
        read: Some([("stops".to_string(), read)].into_iter().collect()),
        now: now.map(|n| [("stops".to_string(), n)].into_iter().collect()),
    };

    let refused = transit_log::receive(&write(stops(&["A"]), None), &[request("R-1", "Z")], &hash, &canonical).unwrap();
    assert!(refused.entry.is_none(), "no entry: {refused:?}");
    assert_eq!(refused.broken.iter().map(|b| b.names.as_str()).collect::<Vec<_>>(), vec!["Z"]);

    let warned = transit_log::receive(&write(stops(&["N"]), Some(stops(&[]))), &[request("R-2", "N")], &hash, &canonical).unwrap();
    assert!(warned.entry.is_some() && warned.broken.is_empty(), "N was there when read: {warned:?}");
    assert_eq!(warned.references.iter().map(|b| (b.names.as_str(), b.entry.as_ref().map(|e| e.seq))).collect::<Vec<_>>(), vec![("N", Some(1))]);

    let unjudged = transit_log::Write { references: Some(kinds()), ..write(stops(&[]), None) };
    let unjudged = transit_log::Write { read: Some(BTreeMap::new()), now: Some(BTreeMap::new()), ..unjudged };
    let landed = transit_log::receive(&unjudged, &[request("R-3", "Z")], &hash, &canonical).unwrap();
    assert!(landed.entry.is_some() && landed.broken.is_empty() && landed.references.is_empty(), "the stops log is not judged: {landed:?}");
}

/// What compaction keeps (D-231): `named` is each record of a target kind
/// that a held record of the given folds names, through a declared
/// reference, in any state. A filtered path names only its own kind, and a
/// fold not given names nothing.
#[test]
fn named_is_every_target_a_held_record_names_in_any_state() {
    let notes = records(&[("note", json!({
        "n1": {"target": {"kind": "alert", "id": "a1"}, "status": "retired"},
        "n2": {"target": {"kind": "alert", "id": "a9"}},
        "n3": {"target": {"kind": "stop", "stop": "a2"}}
    }))]);
    let folds: BTreeMap<String, Records> = [("notes".to_string(), notes)].into_iter().collect();
    let named = transit_log::named(&kinds(), &folds, "service_alert");
    assert_eq!(named.into_iter().collect::<Vec<_>>(), vec!["a1".to_string(), "a9".to_string()], "n3 names a stop, not an alert");
    assert!(transit_log::named(&kinds(), &BTreeMap::new(), "service_alert").is_empty(), "no fold given names nothing");
}
