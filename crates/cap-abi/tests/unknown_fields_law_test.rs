//! Every authored kind keeps the fields it does not know. Properties:
//!
//! 1. Every record's schema takes unknown fields (`additionalProperties:
//!    true`). A value type (`VALUES`) takes none.
//! 2. An unknown field written into every record of a real document survives
//!    a typed read and a write, so an older build does not drop what a newer
//!    one wrote.

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

/// Types that are not records.
const VALUES: &[&str] = &["AlignmentPoint", "Coord", "StopOffset", "WeekdaySet", "DetourPoint"];

const UNKNOWN: &str = "zz_from_a_newer_build";

fn schema<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap()
}

fn resolve<'a>(root: &'a Value, node: &'a Value) -> &'a Value {
    match node.get("$ref").and_then(Value::as_str) {
        Some(r) => {
            let name = r.rsplit('/').next().unwrap();
            resolve(root, &root["$defs"][name])
        }
        None => node,
    }
}

fn is_record(node: &Value) -> bool {
    node.get("properties").is_some() && node.get("additionalProperties") == Some(&Value::Bool(true))
}

fn object_defs(root: &Value) -> Vec<(String, bool)> {
    let mut out = vec![];
    if let Some(defs) = root.get("$defs").and_then(Value::as_object) {
        for (name, def) in defs {
            if def.get("properties").is_some() {
                out.push((name.clone(), is_record(def)));
            }
        }
    }
    out
}

/// An enum's alternatives are visited where the object fits them.
fn inject(root: &Value, node: &Value, doc: &mut Value, count: &mut usize) {
    let node = resolve(root, node);
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(alts) = node.get(key).and_then(Value::as_array) {
            for alt in alts {
                let alt = resolve(root, alt);
                if fits(alt, doc) {
                    inject(root, alt, doc, count);
                    if key != "allOf" {
                        break;
                    }
                }
            }
        }
    }
    match doc {
        Value::Object(map) => {
            if let Some(props) = node.get("properties").and_then(Value::as_object) {
                for (k, v) in map.iter_mut() {
                    if let Some(p) = props.get(k) {
                        inject(root, p, v, count);
                    }
                }
                if is_record(node) && !map.contains_key(UNKNOWN) {
                    map.insert(UNKNOWN.into(), json!({ "kept": true }));
                    *count += 1;
                }
            } else if let Some(values) = node.get("additionalProperties").filter(|v| v.is_object()) {
                for v in map.values_mut() {
                    inject(root, values, v, count);
                }
            }
        }
        Value::Array(items) => {
            if let Some(item) = node.get("items") {
                for v in items {
                    inject(root, item, v, count);
                }
            }
        }
        _ => {}
    }
}

fn fits(alt: &Value, doc: &Value) -> bool {
    let Some(map) = doc.as_object() else { return alt.get("type").and_then(Value::as_str) != Some("object") };
    let required = alt.get("required").and_then(Value::as_array).map(|r| r.iter().filter_map(Value::as_str).all(|k| map.contains_key(k))).unwrap_or(true);
    let consts = alt.get("properties").and_then(Value::as_object).map(|props| {
        props.iter().all(|(k, p)| match p.get("const") {
            Some(c) => map.get(k) == Some(c),
            None => true,
        })
    });
    required && consts.unwrap_or(true) && alt.get("properties").is_some()
}

fn keeps_unknown_fields<T: Serialize + DeserializeOwned + schemars::JsonSchema>(kind: &str, doc: &T, at_least: usize) {
    let root = schema::<T>();
    for (name, record) in object_defs(&root) {
        let value = VALUES.contains(&name.as_str());
        assert!(record != value, "{kind}: {name} {}", if value { "is a value and must take no unknown fields" } else { "is a record and must take unknown fields (an `extra`)" });
    }
    assert!(is_record(&root), "{kind}: the document's root is a record");

    let (_, unknown) = transit_version::noting_unknown(|| serde_json::from_value::<T>(serde_json::to_value(doc).unwrap()).unwrap());
    assert!(unknown.is_empty(), "{kind}: the fixture itself carries fields this build does not know: {unknown:?}");
    let mut written = serde_json::to_value(doc).unwrap();
    let mut count = 0;
    inject(&root, &root, &mut written, &mut count);
    assert!(count >= at_least, "{kind}: the unknown field reached {count} records, want at least {at_least}");
    let (read, noted) = transit_version::noting_unknown(|| serde_json::from_value::<T>(written.clone()));
    let read = read.unwrap_or_else(|e| panic!("{kind}: an unknown field must not be refused: {e}"));
    assert_eq!(noted, vec![UNKNOWN.to_string()], "{kind}: the read names the field it does not know");
    assert_eq!(serde_json::to_value(&read).unwrap(), written, "{kind}: every unknown field survives a read and a write");
}

fn typed<T: DeserializeOwned>(v: Value) -> T {
    serde_json::from_value(v).unwrap()
}

#[test]
fn the_network_keeps_what_it_does_not_know() {
    let zip = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    let feed = transit_codec::Feed::from_zip_bytes(&zip).unwrap();
    let network = transit_codec::import(&feed).network;
    keeps_unknown_fields("network", &network, 50);
}

#[test]
fn the_edit_document_keeps_what_it_does_not_know() {
    let doc: transit_sync::EditDocument = typed(json!({"kind": "edit_records", "schema_version": 2, "records": [
        {"id": "e1", "rev": 1, "status": "accepted", "updated_at": 1, "diversion": {"id": "d1", "route_id": "10", "start_date": "2026-10-23", "end_date": "2026-10-23",
            "first_kept_stop": "A", "last_kept_stop": "B", "skipped_stops": ["C"], "added_secs": 300, "detour": [{"lat": 45.6, "lon": -122.6}, {"lat": 45.61, "lon": -122.61}]}},
        {"id": "e2", "rev": 1, "status": "accepted", "updated_at": 2, "cancel": {"id": "c1", "trip_id": "t1", "date": "2026-10-23"}}
    ]}));
    keeps_unknown_fields("edit_records", &doc, 4);
}

#[test]
fn the_published_plans_keep_what_they_do_not_know() {
    let doc: transit_plans::PublishedPlans = typed(json!({"kind": "plans", "schema_version": 1, "plans": [
        {"id": "spring", "name": "Spring 2026", "effective_from": "2026-03-01", "network_hash": "sha256:00", "status": "published"}]}));
    keeps_unknown_fields("plans", &doc, 2);
}

#[test]
fn the_policy_keeps_what_it_does_not_know() {
    let doc: transit_authz::Policy = typed(json!({"kind": "policy", "schema_version": 1, "rules": [
        {"name": "ops-write-edits", "actions": ["write"], "pointers": ["edits"], "roles": ["dispatcher"]}]}));
    keeps_unknown_fields("policy", &doc, 2);
}

#[test]
fn the_work_rules_keep_what_they_do_not_know() {
    let doc: transit_runs::WorkRules = typed(json!({"kind": "work_rules", "schema_version": 3, "name": "Fixture rules",
        "source": {"title": "An agreement", "effective_from": "2022-01-01"},
        "measures": {"worked": ["report", "revenue"]},
        "allowances": [{"event": "pull_out", "secs": 600, "activity": "report"}],
        "gaps": [{"from_secs": 0, "paid_pct": 100}],
        "limits": [{"quantity": "spread", "max": 54000}],
        "breaks": [{"min_secs": 1800, "window_from_secs": 7200, "window_to_secs": 18000}],
        "premiums": [{"on": "measure", "measure": "worked", "after_secs": 28800, "pays": "worked", "pct": 150}],
        "guarantees": [{"per": "report", "min_secs": 7200, "counts": "worked"}],
        "travel": [{"at": "day_end", "beyond_m": 2414, "within_m": 402, "timed": "next_trip"}],
        "classes": [{"id": "B", "conditions": [{"quantity": "pieces", "min": 2}], "guarantees": [{"per": "day", "min_secs": 28800, "counts": "worked"}]}],
        "mix": [{"classes": ["B"], "max_pct": 30}],
        "objective": {"maximize": ["B"]}}));
    keeps_unknown_fields("work_rules", &doc, 14);
}

#[test]
fn the_scheduling_constraints_keep_what_they_do_not_know() {
    let doc: transit_runs::SchedulingConstraints = typed(json!({"kind": "scheduling_constraints", "schema_version": 1,
        "name": "Fixture constraints",
        "interline": [{"a": "10", "b": "20"}],
        "recovery": [{"min_secs": 300, "pct_of_run": 10}, {"min_secs": 240, "route": "10"}],
        "relief_points": ["A"]}));
    keeps_unknown_fields("scheduling_constraints", &doc, 4);
}

#[test]
fn the_stops_keep_what_they_do_not_know() {
    let doc: transit_stops::Stops = typed(json!({"kind": "stops", "schema_version": 1, "stops": {
        "6124": [{"on": "2004-09-01", "event": "in_service", "lat": 45.6934, "lon": -122.6512, "name": "NE 99th St & Hwy 99",
            "attachments": [{"hash": "sha256:00", "type": "image/jpeg"}]}]}}));
    keeps_unknown_fields("stops", &doc, 3);
}

#[test]
fn the_stop_requests_keep_what_they_do_not_know() {
    let doc: transit_requests::StopRequests = typed(json!({"kind": "stop_requests", "schema_version": 2, "requests": {
        "R-1": {"stop": "7001", "job": "new", "status": "ordered", "requested_by": "Planning", "requested_on": "2027-01-04",
            "locate": {"ticket": "26123456", "called_on": "2027-01-06", "utilities": ["NW Natural"]},
            "work_order": {"system": "Maintenance", "id": "WO-1", "status": "open", "read_on": "2027-01-07"}}}}));
    keeps_unknown_fields("stop_requests", &doc, 4);
}
