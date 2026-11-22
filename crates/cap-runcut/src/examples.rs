//! The runcut capability's worked examples (its `examples` feature): the
//! documents to load and each request, over the fixture Friday. The
//! generator runs them through this crate's `invoke` and records the
//! answers.

use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let rules = fixtures::fixture_work_rules().checked().expect("the fixture work rules pass");
    let records = fixtures::dispatch_fixture_edits();
    let edits = transit_sync::EditDocument::new(records.clone());
    let hash = |d: &dyn Hashed| d.hashed();
    let network = || fixtures::load_network(&net.derived);
    let with_rules = || vec![network(), fixtures::load("load_work_rules", "work_rules", &rules)];
    let with_edits = || vec![network(), fixtures::load("load_edits", "edits", &edits)];
    // t30: the via-C run at 08:40 — the patched day keeps A→B (C skipped,
    // +300 s), and draws the diversion's line.
    let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).expect("a date");
    let day = transit_dispatch::effective_day(&net.network, date, &records).expect("the patched day");
    let diverted = day.iter().find(|t| t.stop_times.len() == 2 && t.start_secs == 8 * 3600 + 40 * 60).expect("the diverted trip");
    let on_day = |op: &'static str, loads: Vec<serde_json::Value>, extra: serde_json::Value| {
        let mut request = json!({ "op": op, "network_hash": net.network_hash, "date": "2026-10-23" });
        for (k, v) in extra.as_object().expect("an object") {
            request[k] = v.clone();
        }
        Example { bytes: None, kinds: vec![], op, loads, request }
    };
    let trip = json!({ "edits_hash": hash(&edits), "trip": diverted.trip, "start_secs": diverted.start_secs });
    vec![
        on_day("recovery", with_rules(), json!({ "work_rules_hash": hash(&rules) })),
        on_day("suggest_blocks", with_rules(), json!({ "work_rules_hash": hash(&rules) })),
        on_day("blocks", vec![network()], json!({})),
        on_day("fleet_summary", vec![network()], json!({})),
        on_day("block_trips", with_edits(), json!({ "edits_hash": hash(&edits), "block_id": "B1" })),
        on_day("trip_stops", with_edits(), trip.clone()),
        on_day("trip_path", with_edits(), trip),
    ]
}

/// A fixture document's canonical hash, whatever its type.
trait Hashed {
    fn hashed(&self) -> String;
}

impl<T: serde::Serialize> Hashed for T {
    fn hashed(&self) -> String {
        cap_abi::canonical_hash(self).expect("a fixture document hashes")
    }
}
