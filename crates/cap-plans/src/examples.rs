//! The plan chain's worked examples (its `examples` feature): each
//! request, over the fixture's two-pick chain. The generator runs them
//! through this crate's `invoke` and records the answers.

use cap_abi::manifest::Example;
use serde_json::{json, Value};
use transit_plans::{DevicePlan, FrozenVersions, PublishedPlans};

fn ex(op: &'static str, request: Value) -> Example {
    Example { bytes: None, kinds: vec![], op, loads: vec![], request }
}

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let hash = net.network_hash.clone();
    let plan_set = fixtures::fixture_plan_set();
    let first = &plan_set[0];
    let today = fixtures::fixture_today();
    // Both picks published over the fixture network.
    let published = plan_set.iter().fold(PublishedPlans::empty(), |set, plan| transit_plans::publish(&set, plan, &hash, today).unwrap_or(set));
    // The first pick alone, as the plans log holds it.
    let one = transit_plans::publish(&PublishedPlans::empty(), first, &hash, today).expect("the first pick publishes");
    let records = transit_log::document_records(&serde_json::to_value(&one).expect("a document"), "plans", "pick", "plans", 1).expect("its records");
    let held = |k: &transit_log::KindRecords| -> serde_json::Map<String, Value> {
        k.records.iter().map(|(id, v)| (id.clone(), json!({ "version": k.version, "value": v }))).collect()
    };
    let frozen = transit_plans::freeze(&FrozenVersions::empty(), first, &hash, "board packet").expect("the first pick freezes");
    // A device holding the fall pick and a draft; the winter pick is
    // published and not on the device yet.
    let set: PublishedPlans = serde_json::from_value(json!({
        "kind": "plans", "schema_version": transit_plans::PUBLISHED_SCHEMA_VERSION,
        "plans": [
            { "id": "fall", "name": "Fall 2026", "effective_from": "2026-10-19", "network_hash": "sha256:…", "status": "published" },
            { "id": "winter", "name": "Winter 2027", "effective_from": "2027-01-04", "network_hash": "sha256:…", "status": "published" },
        ]
    }))
    .expect("the example set reads");
    let device = vec![
        DevicePlan { key: "plan-fall".into(), id: "fall".into(), name: "Fall 2026".into(), effective_from: Some("2026-10-19".into()), served: false },
        DevicePlan { key: "plan-draft".into(), id: "spring".into(), name: "Spring 2027 draft".into(), effective_from: Some("2027-03-01".into()), served: false },
    ];
    let mut write = fixtures::receive_write("plans", "planner-1", 1_792_752_000_000);
    write.header.intent.op = "POST /plans".into();
    vec![
        ex("resolve", json!({ "op": "resolve", "plans": plan_set, "date": "2026-10-23" })),
        ex("validate", json!({ "op": "validate", "plans": plan_set })),
        ex("publish", json!({ "op": "publish", "published": null, "plan": first, "network_hash": hash, "today": "2026-01-01" })),
        ex("in_force", json!({ "op": "in_force", "published": published, "date": "2026-10-23" })),
        ex("in_use", json!({ "op": "in_use", "published": published, "today": "2026-10-23" })),
        ex("retire", json!({ "op": "retire", "published": published, "id": "fall", "today": "2026-01-01" })),
        ex("freeze", json!({ "op": "freeze", "frozen": null, "plan": first, "network_hash": hash, "purpose": "proposal for public comment" })),
        ex("adopt", json!({ "op": "adopt", "published": null, "frozen": frozen, "network_hash": hash, "today": "2026-01-01", "stops_seq": 3 })),
        ex("log_records", json!({ "op": "log_records", "published": one })),
        // A publish in an empty plans log: the network frozen as the
        // published pick, then adopted, in one entry.
        ex("receive", json!({ "op": "receive", "write": write, "ops": [
            { "cap": "plans", "op": "freeze", "args": { "plan": first, "network_hash": hash, "purpose": "the published pick" } },
            { "cap": "plans", "op": "adopt", "args": { "network_hash": hash, "today": "2026-01-01" } }
        ] })),
        ex("log_document", json!({ "op": "log_document", "kind": "plans", "records": held(&records[0]), "document": held(&records[1]).remove("document") })),
        ex("timeline", json!({ "op": "timeline", "plans": device, "published": set, "today": "2026-11-13" })),
        ex("periods", json!({ "op": "periods", "plans": plan_set })),
    ]
}
