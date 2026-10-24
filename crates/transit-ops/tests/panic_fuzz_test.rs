//! Panic-fuzz properties: the artifact takes untrusted bytes at three
//! boundaries (GTFS zips, GTFS-rt protobufs, ABI request payloads).
//! Panics are wasm traps — the "errors are values" contract means no
//! input may ever panic, only return errors. These proptests hold that
//! line without needing a fuzzer harness: arbitrary bytes in, outcome
//! or error out, never a panic.

use proptest::prelude::*;

/// Arbitrary bytes, biased toward small inputs (most malformed feeds
/// fail fast) but including chunky ones.
fn arbitrary_bytes() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..=4096)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// The zip parser must never panic on arbitrary bytes.
    #[test]
    fn zip_parser_never_panics(bytes in arbitrary_bytes()) {
        let _ = transit_core::Gtfs::from_zip_bytes(&bytes);
    }

    /// The GTFS-rt decoder must never panic on arbitrary bytes.
    #[test]
    fn rt_decoder_never_panics(bytes in arbitrary_bytes()) {
        let _ = transit_rt::decode(&bytes);
    }

    /// dispatch_json must never panic on arbitrary text — any string
    /// yields an outcome, malformed or not.
    #[test]
    fn dispatch_json_never_panics(input in "\\PC*") {
        let outcome = transit_ops::dispatch_json(&input);
        // Every outcome parses back as JSON — the wrapper is total too.
        let parsed: serde_json::Value = serde_json::from_str(&outcome)
            .expect("dispatch_json must always emit valid JSON");
        prop_assert!(
            parsed.get("ok").is_some() ^ parsed.get("error").is_some(),
            "outcome must be exactly one of ok/error: {outcome}"
        );
    }

    /// dispatch_bytes must never panic on arbitrary bytes or op codes
    /// (unknown codes included — hosts could pass anything).
    #[test]
    fn dispatch_bytes_never_panics(op in any::<u32>(), bytes in arbitrary_bytes()) {
        let outcome = transit_ops::dispatch_bytes(op, &bytes);
        let parsed: serde_json::Value = serde_json::from_str(&outcome)
            .expect("dispatch_bytes must always emit valid JSON");
        prop_assert!(parsed.get("ok").is_some() ^ parsed.get("error").is_some());
    }
}
