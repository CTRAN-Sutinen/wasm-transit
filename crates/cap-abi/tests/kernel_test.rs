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
