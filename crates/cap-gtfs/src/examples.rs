use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    vec![Example {
        bytes: None,
        kinds: vec![],
        op: "feed_info",
        loads: vec![fixtures::load_feed()],
        request: json!({ "op": "feed_info", "feed_hash": fixtures::feed_hash() }),
    }]
}
