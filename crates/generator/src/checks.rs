//! The generated gen-manifest crate runs these checks as its tests, over the
//! capabilities it links ([`crate::Linked`]), so this code names no
//! capability. The schemas are the locks (docs/FORMATS.md, "The schema
//! lock").
//!
//! [`real_output`] runs every capability's worked examples through the
//! capability's own dispatch over the e2e fixture feed. It checks every
//! document in every answer against the locked schema of its tag and
//! version, which is where `schemars` and serde can disagree
//! (`skip_serializing_if`, `untagged`, `flatten`).
//!
//! [`soak`] runs the agency's own feed through the ops that a server runs on
//! it (`just soak` sets SOAK_FEED to `testdata/ctran-gtfs.zip`). Hermetic runs
//! skip it. A mismatch is a QA finding.

use std::path::Path;

use serde_json::{json, Value};

use crate::doc_check::Locks;
use crate::Linked;

use cap_abi::kernel::Load;

/// The one capability that answers the op and declares the kind.
fn lanes(caps: &[Linked], op: &str, reads: &str) -> (fn(&str) -> String, fn(u32, &[u8]) -> String) {
    let found: Vec<&Linked> = caps
        .iter()
        .filter(|c| c.declared.ops.iter().any(|o| o == op) && c.declared.documents.contains(&reads))
        .collect();
    let [linked] = found[..] else { panic!("{} capabilities answer {op} over a {reads}, not one", found.len()) };
    (linked.declared.invoke, linked.declared.invoke_bytes)
}

pub fn real_output(caps: &[Linked]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("web/public/caps/manifest.json")).expect("the manifest (run `just caps`)"),
    )
    .unwrap();
    let feed = std::fs::read(root.join("web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    let rt = std::fs::read(root.join("web/e2e/fixtures/sample-rt.pb")).unwrap();
    // The generator imports the same street fixtures.
    let load_streets: Vec<String> = ["streets.geojson", "walk-streets.geojson"]
        .iter()
        .map(|file| {
            let layer = std::fs::read(root.join("web/e2e/fixtures").join(file)).unwrap();
            let streets = transit_streets::geojson::import(&layer).unwrap();
            json!({"op": "load_streets", "streets": streets}).to_string()
        })
        .collect();
    let loads: Vec<(&str, &str)> =
        cap_abi::kinds::KINDS.iter().filter_map(|k| k.load).map(|l| (l.op, l.id)).collect();
    let locks = Locks::read();

    let mut failures = Vec::new();
    let mut checked = std::collections::BTreeMap::<String, usize>::new();
    // Each kind that a recorded answer holds, with the first op that holds
    // it. A real answer must hold it too, so a setup that leaves an op
    // unanswered cannot pass by checking nothing.
    let mut promised = std::collections::BTreeMap::<String, String>::new();
    for cap in manifest["capabilities"].as_array().unwrap() {
        let id = cap["id"].as_str().unwrap();
        let linked = caps.iter().find(|c| c.declared.id == id).unwrap_or_else(|| panic!("capability {id} is not linked"));
        let (invoke, invoke_bytes) = (linked.declared.invoke, linked.declared.invoke_bytes);
        let reads: Vec<&str> = cap["documents"].as_array().map(|d| d.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        for (name, kind) in &loads {
            let load = manifest["kernel_ops"].as_array().unwrap().iter().find(|op| op["name"] == *name);
            if let (true, Some(request)) = (reads.contains(kind), load.and_then(|op| op["example"].get("request"))) {
                invoke(&request.to_string());
            }
        }
        if reads.contains(&"feed") {
            invoke_bytes(transit_codec::Feed::CODE, &feed);
        }
        if reads.contains(&"rt") {
            invoke_bytes(transit_rt::RtOverview::CODE, &rt);
        }
        if reads.contains(&"streets") {
            for load in &load_streets {
                invoke(load);
            }
        }
        for (op, doc) in cap["op_docs"].as_object().into_iter().flatten() {
            let Some(request) = doc["example"].get("request") else { continue };
            let answer: Value = match serde_json::from_str(&invoke(&request.to_string())) {
                Ok(answer) => answer,
                Err(e) => {
                    failures.push(format!("{id}.{op}: the answer is not JSON ({e})"));
                    continue;
                }
            };
            for (key, _) in locks.check(&doc["example"]["response"]) {
                promised.entry(key).or_insert_with(|| format!("{id}.{op}"));
            }
            for (key, problems) in locks.check(&answer) {
                *checked.entry(key.clone()).or_default() += 1;
                if !problems.is_empty() {
                    failures.push(format!("{id}.{op}: its {key} document does not match its schema: {}", problems.join("; ")));
                }
            }
        }
    }
    for (key, op) in &promised {
        if !checked.contains_key(key) {
            failures.push(format!(
                "{op}'s worked example answers a {key} document, and no real answer here held one: the setup left it unanswered, \
                 or the manifest is older than the code (run `just caps`)"
            ));
        }
    }
    assert!(failures.is_empty(), "documents that do not match their schemas:\n  {}", failures.join("\n  "));
    assert!(!checked.is_empty(), "no answer held a document: the gate checked nothing");
    println!("real output: {}", json!(checked));
}

fn ok(answer: String, what: &str) -> Value {
    let answer: Value = serde_json::from_str(&answer).unwrap_or_else(|e| panic!("{what}: not JSON ({e})"));
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{what}: {answer}"))
}

pub fn soak(caps: &[Linked]) {
    let Ok(feed) = std::env::var("SOAK_FEED") else {
        eprintln!("skipped: set SOAK_FEED to a GTFS zip (just soak)");
        return;
    };
    let zip = std::fs::read(&feed).unwrap_or_else(|e| panic!("{feed}: {e}"));
    let locks = Locks::read();
    let (network, network_bytes) = lanes(caps, "import", "feed");
    let (plans, _) = lanes(caps, "publish", "network");

    let loaded = ok(network_bytes(transit_codec::Feed::CODE, &zip), "load the feed");
    let import = ok(network(&json!({"op": "import", "feed_hash": loaded["feed_hash"]}).to_string()), "import");
    let publish = ok(
        plans(
            &json!({
                "op": "publish",
                "network_hash": import["network_hash"],
                "plan": {"id": "soak", "name": "Soak pick", "effective_from": "2026-09-01"},
                "published": null,
                "today": "2026-08-01",
            })
            .to_string(),
        ),
        "publish",
    );

    let mut failures = Vec::new();
    let mut checked = Vec::new();
    for (what, answer) in [("import", &import), ("publish", &publish)] {
        for (key, problems) in locks.check(answer) {
            if !problems.is_empty() {
                failures.push(format!("{what}: its {key} document does not match its schema: {}", problems.join("; ")));
            }
            checked.push(key);
        }
    }
    assert!(failures.is_empty(), "{feed}: documents that do not match their schemas:\n  {}", failures.join("\n  "));
    for want in ["network v2", "boundary v1", "plans v1"] {
        assert!(checked.iter().any(|k| k == want), "the soak checked no {want} document: {checked:?}");
    }
    println!("soak: {feed}: {checked:?} match their schemas");
}
