//! Golden corpus replay (testdata/corpus.json): the same
//! request→wrapper-text pairs that the Go host and the Node smoke
//! replay. Every host must produce byte-identical `{\"ok\":...}` /
//! `{\"error\":...}` strings from the same artifact bytes — this is the
//! cross-runtime equivalence proof. Expected values are generated
//! from the artifact (`just corpus`), never hand-written.

use std::fs;

use serde_json::Value;

#[test]
fn golden_corpus_replays_byte_identically() {
    let root = format!("{}/../..", env!("CARGO_MANIFEST_DIR"));
    let corpus_text = fs::read_to_string(format!("{root}/testdata/corpus.json"))
        .expect("testdata/corpus.json exists (run `just corpus`)");
    let corpus: Value = serde_json::from_str(&corpus_text).unwrap();
    let fixture =
        fs::read(format!("{root}/{}", corpus["fixture"].as_str().unwrap())).expect("fixture zip");

    let mut feed_id = String::new();
    let mut net_feed_id = String::new();
    for step in corpus["steps"].as_array().expect("steps") {
        let name = step["name"].as_str().unwrap();
        let actual = match step["bytes_op"].as_u64() {
            // The bulk-bytes step (feed load): the registry assigns
            // the id, captured from the response and substituted into
            // every later step — so the corpus replays regardless of
            // how many feeds earlier tests registered.
            Some(op) => {
                let outcome = transit_ops::dispatch_bytes(op as u32, &fixture);
                let parsed: Value = serde_json::from_str(&outcome).unwrap();
                feed_id = parsed["ok"]["feed_id"].as_u64().unwrap().to_string();
                outcome
            }
            None => {
                let request = step["request"].as_str().unwrap();
                assert!(
                    !request.contains("$FEED") || !feed_id.is_empty(),
                    "step {name} uses $FEED before the load step"
                );
                assert!(
                    !request.contains("$NET") || !net_feed_id.is_empty(),
                    "step {name} uses $NET before the network_load step"
                );
                transit_ops::dispatch_json(&request.replace("$FEED", &feed_id).replace("$NET", &net_feed_id))
            }
        };
        // A step with `bind` registers a second feed (the network
        // document); its id is captured here, exactly like the bytes
        // load above.
        if step["bind"].as_str() == Some("$NET") {
            let parsed: Value = serde_json::from_str(&actual).unwrap();
            net_feed_id = parsed["ok"]["feed_id"].as_u64().unwrap().to_string();
        }
        let expected = step["expect"]
            .as_str()
            .unwrap()
            .replace("$FEED", &feed_id)
            .replace("$NET", &net_feed_id);
        assert_eq!(actual, expected, "corpus step {name} diverges");
    }
}
