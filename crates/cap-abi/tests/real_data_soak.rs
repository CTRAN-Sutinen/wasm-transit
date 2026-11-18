//! The real-data soak of the schemas (docs/FORMATS.md, "The schema
//! lock"; `just soak`): the agency's own feed through the ops a server
//! runs on it, natively (each capability's own dispatch), and every
//! document they answer checked against its locked schema. The import
//! answers the network and its boundary, and publishing a pick of it
//! answers the published plans. Hermetic runs skip it. Set SOAK_FEED to
//! the feed (`just soak` sets it to testdata/ctran-gtfs.zip). A mismatch
//! is a QA finding.

#[path = "common/doc_check.rs"]
mod doc_check;

use serde_json::{json, Value};

const OP_LOAD_FEED: u32 = 266;

fn ok(answer: String, what: &str) -> Value {
    let answer: Value = serde_json::from_str(&answer).unwrap_or_else(|e| panic!("{what}: not JSON ({e})"));
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{what}: {answer}"))
}

#[test]
fn the_agencys_feed_answers_documents_that_match_their_schemas() {
    let Ok(feed) = std::env::var("SOAK_FEED") else {
        eprintln!("skipped: set SOAK_FEED to a GTFS zip (just soak)");
        return;
    };
    let zip = std::fs::read(&feed).unwrap_or_else(|e| panic!("{feed}: {e}"));
    let locks = doc_check::Locks::read();

    let loaded = ok(cap_network::invoke_bytes(OP_LOAD_FEED, &zip), "load the feed");
    let import = ok(
        cap_network::invoke(&json!({"op": "import", "feed_hash": loaded["feed_hash"]}).to_string()),
        "import",
    );
    let publish = ok(
        cap_plans::invoke(
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
    for (what, answer) in [("network.import", &import), ("plans.publish", &publish)] {
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
