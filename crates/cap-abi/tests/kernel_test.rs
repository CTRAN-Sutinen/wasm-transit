//! A document's identity is the content hash of its canonical serialization,
//! and streaming that serialization into the digest changes no hash. The
//! versioned kinds read and refuse versions as docs/FORMATS.md, "Versions",
//! says.

use proptest::prelude::*;
use sha2::Digest;

#[derive(Debug, Clone, serde::Serialize)]
struct Doc {
    name: String,
    rows: Vec<(String, i64, Option<f64>)>,
}

fn gen_doc() -> impl Strategy<Value = Doc> {
    (".*", prop::collection::vec((".*", any::<i64>(), prop::option::of(-1e9f64..1e9)), 0..40))
        .prop_map(|(name, rows)| Doc { name, rows })
}

proptest! {
    #[test]
    fn the_streamed_hash_is_the_hash_of_the_serialization(doc in gen_doc()) {
        let text = serde_json::to_string(&doc).unwrap();
        let expect = format!("sha256:{:x}", sha2::Sha256::digest(text.as_bytes()));
        prop_assert_eq!(cap_abi::canonical_hash(&doc).unwrap(), expect.clone());
        prop_assert_eq!(cap_abi::content_hash(text.as_bytes()), expect);
    }
}

#[test]
fn outcomes_are_the_ok_or_error_envelope() {
    let ok: serde_json::Value = serde_json::from_str(&cap_abi::outcome(Ok::<_, String>(vec![1, 2]))).unwrap();
    assert_eq!(ok, serde_json::json!({ "ok": [1, 2] }));
    let err: serde_json::Value = serde_json::from_str(&cap_abi::outcome(Err::<(), _>("nope".to_string()))).unwrap();
    assert_eq!(err, serde_json::json!({ "error": { "message": "nope" } }));
}

#[test]
fn a_document_that_does_not_parse_is_an_error() {
    let err = cap_abi::parse_document::<Vec<u32>>(b"[1, \"two\"]").unwrap_err();
    assert!(err.starts_with("invalid document"), "{err}");
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    z.write_all(bytes).unwrap();
    z.finish().unwrap()
}

proptest! {
    /// A document crosses as its JSON or that JSON gzipped, with the same
    /// identity.
    #[test]
    fn a_gzipped_document_is_the_same_document(doc in gen_doc()) {
        #[derive(Debug, PartialEq, serde::Deserialize)]
        struct Back { name: String, rows: Vec<(String, i64, Option<f64>)> }
        let text = serde_json::to_vec(&doc).unwrap();
        let plain: Back = cap_abi::parse_document(&text).unwrap();
        let zipped: Back = cap_abi::parse_document(&gzip(&text)).unwrap();
        prop_assert_eq!(&plain, &zipped);
        prop_assert_eq!(plain.name, doc.name);
    }

    /// A gzip stream cut short is an error, never a partial document.
    #[test]
    fn a_truncated_gzip_is_an_error(doc in gen_doc(), cut in 0.0f64..1.0) {
        let zipped = gzip(&serde_json::to_vec(&doc).unwrap());
        let at = ((zipped.len() - 1) as f64 * cut) as usize;
        let err = cap_abi::parse_document::<serde_json::Value>(&zipped[..at.max(2)]).unwrap_err();
        prop_assert!(err.starts_with("invalid document"), "{}", err);
    }
}

#[test]
fn a_gzipped_network_registers_under_the_plain_hash() {
    let text = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/network.json")).unwrap();
    let plain = cap_abi::parse_versioned::<transit_network::Network>(&text).unwrap();
    let zipped = cap_abi::parse_versioned::<transit_network::Network>(&gzip(&text)).unwrap();
    assert_eq!(cap_abi::canonical_hash(&plain).unwrap(), cap_abi::canonical_hash(&zipped).unwrap());
}

/// The live document is ephemeral, so no migration brings a v1 forward.
#[test]
fn the_live_document_reads_v2_only() {
    use transit_version::Versioned;
    assert_eq!(transit_live::LiveDay::READS, 2);
    let body = serde_json::json!({
        "window": {"early_secs": 0, "late_secs": 300, "very_late_secs": 600}, "trips": [], "vehicles": [], "unmatched_trips": 0
    });
    let older = cap_abi::parse_versioned::<transit_live::LiveDay>(body.to_string().as_bytes()).unwrap_err();
    assert!(older.message.contains("live v1") && older.message.contains("reads v2"), "{older}");
    let mut current = body.clone();
    current["kind"] = serde_json::json!("live");
    current["schema_version"] = serde_json::json!(2);
    let read: transit_live::LiveDay = cap_abi::parse_versioned(current.to_string().as_bytes()).unwrap();
    assert_eq!(serde_json::to_value(&read).unwrap()["schema_version"], 2);
    current["schema_version"] = serde_json::json!(3);
    let newer = cap_abi::parse_versioned::<transit_live::LiveDay>(current.to_string().as_bytes()).unwrap_err();
    assert!(newer.message.contains("live v3") && newer.message.contains("reads v2"), "{newer}");
}

/// The check runs at the load, never deep inside an op.
#[test]
fn a_network_load_refuses_other_versions_naming_both() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/network.json")).unwrap();
    let current = transit_network::SCHEMA_VERSION;
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(cap_abi::parse_versioned::<transit_network::Network>(text.as_bytes()).is_ok());

    let root = doc.as_object_mut().unwrap();
    root.remove("schema_version");
    let older = cap_abi::parse_versioned::<transit_network::Network>(doc.to_string().as_bytes()).unwrap_err();
    assert!(older.message.contains("network v1") && older.message.contains(&format!("reads v{current}")) && older.message.contains("migrate"), "{older}");

    doc["schema_version"] = serde_json::json!(current + 1);
    // A newer document's shape may still parse, so its tag must refuse it.
    let newer = cap_abi::parse_versioned::<transit_network::Network>(doc.to_string().as_bytes()).unwrap_err();
    assert!(newer.message.contains(&format!("network v{}", current + 1)) && newer.message.contains(&format!("reads v{current}")), "{newer}");
    doc["stops"] = serde_json::json!("a shape this build does not know");
    let newer = cap_abi::parse_versioned::<transit_network::Network>(doc.to_string().as_bytes()).unwrap_err();
    assert!(newer.message.contains(&format!("network v{}", current + 1)), "{newer}");
}

/// The derived kinds and the work rules gained their tags at v1, so an
/// untagged copy is v1.
#[test]
fn every_versioned_kind_reads_untagged_as_v1_and_refuses_a_newer_copy() {
    use transit_version::Versioned;
    fn law<T: serde::de::DeserializeOwned + serde::Serialize + Versioned + std::fmt::Debug>(untagged: serde_json::Value) {
        let read: T = cap_abi::parse_versioned(untagged.to_string().as_bytes()).unwrap_or_else(|e| panic!("{}: {e}", T::KIND));
        let written = serde_json::to_value(&read).unwrap();
        assert_eq!(written["kind"], T::KIND, "a written copy names its kind");
        assert_eq!(written[T::VERSION_FIELD], 1, "{}: a written copy names its version", T::KIND);
        let mut newer = written.clone();
        newer[T::VERSION_FIELD] = serde_json::json!(T::READS + 1);
        let err = cap_abi::parse_versioned::<T>(newer.to_string().as_bytes()).unwrap_err();
        assert!(
            err.message.contains(&format!("{} v{}", T::KIND, T::READS + 1)) && err.message.contains(&format!("reads v{}", T::READS)),
            "{err}"
        );
        let mut other = written;
        other["kind"] = serde_json::json!("not-this-kind");
        assert!(cap_abi::parse_versioned::<T>(other.to_string().as_bytes()).is_err(), "{}: another kind is refused", T::KIND);
    }
    law::<transit_codec::Boundary>(serde_json::json!({}));
    law::<transit_streets::Streets>(serde_json::json!({"ways": []}));
    law::<transit_streets::walking::WalkTable>(serde_json::json!({
        "streets_hash": "sha256:s", "network_hash": "sha256:n", "speed_mps": 1.3, "max_walk_secs": 600, "walks": []
    }));
    law::<transit_runs::WorkRules>(serde_json::json!({"schema_version": 1}));
    law::<transit_stops::Stops>(serde_json::json!({"stops": {}}));
    law::<transit_plans::PublishedPlans>(serde_json::json!({"schema_version": 1, "plans": []}));
    law::<transit_authz::Policy>(serde_json::json!({"schema_version": 1, "rules": []}));
}

/// Every v1 document is a v2 document, so an untagged copy reads (D-234).
#[test]
fn stop_requests_read_v2_and_refuse_v1_naming_the_migration() {
    use transit_requests::StopRequests;
    let err = cap_abi::parse_versioned::<StopRequests>(br#"{"kind":"stop_requests","schema_version":1,"requests":{}}"#).unwrap_err();
    assert!(err.message.contains("stop_requests v1") && err.message.contains("migrate"), "{err}");
    let untagged: StopRequests = cap_abi::parse_versioned(br#"{"requests":{}}"#).unwrap();
    assert_eq!(serde_json::to_value(&untagged).unwrap()["schema_version"], 2);
    let read: StopRequests = cap_abi::parse_versioned(br#"{"kind":"stop_requests","schema_version":2,"requests":{}}"#).unwrap();
    assert_eq!(serde_json::to_value(&read).unwrap()["schema_version"], 2);
    let err = cap_abi::parse_versioned::<StopRequests>(br#"{"kind":"stop_requests","schema_version":3,"requests":{}}"#).unwrap_err();
    assert!(err.message.contains("stop_requests v3") && err.message.contains("reads v2"), "{err}");
}

/// The watchlist gained its tags at v2. A copy from before them reads by
/// its `version` field.
#[test]
fn the_watchlist_reads_its_v2_from_before_the_tags_and_refuses_every_other_version() {
    use transit_rider::watchlist::Watchlist;
    let read: Watchlist = cap_abi::parse_versioned(br#"{"version":"v2","stops":[{"id":"A","label":"home"}]}"#).unwrap();
    let written = serde_json::to_value(&read).unwrap();
    assert_eq!(written, serde_json::json!({"kind": "watchlist", "schema_version": 2, "stops": [{"id": "A", "label": "home"}]}));
    for older in [r#"{"version":"v1","stops":[{"id":"A","note":"home"}]}"#, r#"{"version":"v1","stops":[]}"#] {
        let err = cap_abi::parse_versioned::<Watchlist>(older.as_bytes()).unwrap_err();
        assert!(err.message.contains("watchlist v1") && err.message.contains("migrate"), "{older}: {err}");
    }
    let err = cap_abi::parse_versioned::<Watchlist>(br#"{"kind":"watchlist","schema_version":3,"stops":[]}"#).unwrap_err();
    assert!(err.message.contains("watchlist v3") && err.message.contains("reads v2"), "{err}");
    assert!(cap_abi::parse_versioned::<Watchlist>(br#"{"kind":"stops","schema_version":2,"stops":[]}"#).is_err());
}
