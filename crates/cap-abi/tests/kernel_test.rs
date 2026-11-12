//! The kernel helpers every capability shares. The law: a document's
//! identity is the content hash of its canonical serialization, and
//! streaming that serialization into the digest changes nothing —
//! every hash minted before the stream is the same hash after it.

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
    assert_eq!(err, serde_json::json!({ "error": "nope" }));
}

#[test]
fn a_document_that_does_not_parse_is_an_error() {
    let err = cap_abi::parse_document::<Vec<u32>>(b"[1, \"two\"]").unwrap_err();
    assert!(err.starts_with("invalid document"), "{err}");
}

/// gzip a document's bytes the way a host serves them.
fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    z.write_all(bytes).unwrap();
    z.finish().unwrap()
}

proptest! {
    /// A document crosses as its JSON or that JSON gzipped, and both
    /// are the same document: the same value, so the same identity.
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
    let plain = cap_abi::parse_network(&text).unwrap();
    let zipped = cap_abi::parse_network(&gzip(&text)).unwrap();
    assert_eq!(cap_abi::canonical_hash(&plain).unwrap(), cap_abi::canonical_hash(&zipped).unwrap());
}

/// The live document is v2 (a trip instance carries its service
/// date): an untagged copy is its v1 and is refused naming both
/// versions, as is a newer one; a v2 copy reads. It is ephemeral —
/// minted by the build that reads it — so no migration brings a v1
/// forward.
#[test]
fn the_live_document_reads_v2_only() {
    use transit_version::Versioned;
    assert_eq!(transit_live::LiveDay::READS, 2);
    let body = serde_json::json!({
        "window": {"early_secs": 0, "late_secs": 300, "very_late_secs": 600}, "trips": [], "vehicles": [], "unmatched_trips": 0
    });
    let older = cap_abi::parse_versioned::<transit_live::LiveDay>(body.to_string().as_bytes()).unwrap_err();
    assert!(older.contains("live v1") && older.contains("reads v2"), "{older}");
    let mut current = body.clone();
    current["kind"] = serde_json::json!("live");
    current["schema_version"] = serde_json::json!(2);
    let read: transit_live::LiveDay = cap_abi::parse_versioned(current.to_string().as_bytes()).unwrap();
    assert_eq!(serde_json::to_value(&read).unwrap()["schema_version"], 2);
    current["schema_version"] = serde_json::json!(3);
    let newer = cap_abi::parse_versioned::<transit_live::LiveDay>(current.to_string().as_bytes()).unwrap_err();
    assert!(newer.contains("live v3") && newer.contains("reads v2"), "{newer}");
}

/// A module reads the network version it was built for, and only that
/// (docs/units/document-versioning.md, rule 5): an older document is
/// refused with the op that brings it forward, a newer one naming both
/// versions — at the load, never three layers deep in an op.
#[test]
fn a_network_load_refuses_other_versions_naming_both() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/network.json")).unwrap();
    let current = transit_network::SCHEMA_VERSION;
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(cap_abi::parse_network(text.as_bytes()).is_ok());

    let root = doc.as_object_mut().unwrap();
    root.remove("schema_version");
    let older = cap_abi::parse_network(doc.to_string().as_bytes()).unwrap_err();
    assert!(older.contains("network v1") && older.contains(&format!("reads v{current}")) && older.contains("migrate"), "{older}");

    doc["schema_version"] = serde_json::json!(current + 1);
    // A newer document's shape may still parse; its tag refuses it.
    let newer = cap_abi::parse_network(doc.to_string().as_bytes()).unwrap_err();
    assert!(newer.contains(&format!("network v{}", current + 1)) && newer.contains(&format!("reads v{current}")), "{newer}");
    doc["stops"] = serde_json::json!("a shape this build does not know");
    let newer = cap_abi::parse_network(doc.to_string().as_bytes()).unwrap_err();
    assert!(newer.contains(&format!("network v{}", current + 1)), "{newer}");
}

/// Every versioned kind's load (docs/FORMATS.md, "Versions"): a copy
/// with no tag is the kind's v1 (the derived kinds and the work rules
/// gained their tags at v1), and a newer copy is refused naming both
/// versions — never read as this build's.
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
            err.contains(&format!("{} v{}", T::KIND, T::READS + 1)) && err.contains(&format!("reads v{}", T::READS)),
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
    law::<transit_runs::WorkRules>(serde_json::json!({"version": 1}));
    law::<transit_stops::Stops>(serde_json::json!({"stops": {}}));
    law::<transit_plans::PublishedPlans>(serde_json::json!({"schema_version": 1, "plans": []}));
    law::<transit_authz::Policy>(serde_json::json!({"schema_version": 1, "rules": []}));
}
