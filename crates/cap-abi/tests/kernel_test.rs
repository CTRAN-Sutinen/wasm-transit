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
