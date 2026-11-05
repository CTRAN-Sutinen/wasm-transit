//! Native dispatch tests for the gtfs capability — the seam without
//! wasm (the same law the monolith's dispatch_test holds). The
//! content-hash registry gets explicit laws: same bytes, same hash,
//! one entry (idempotence); different bytes, different hashes; and
//! the registry is addressed by hash, so a query for an unknown hash
//! is an error, never a panic. The day views AND the directory
//! questions (pickers, names join) are NOT here — they answer over
//! the NETWORK document (cap-board / cap-runcut / cap-calendar /
//! cap-directory; one question, one answer on the bus).

use std::io::Read;

fn fixture_zip() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../web/e2e/fixtures/sample-gtfs.zip");
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .expect("fixture exists")
        .read_to_end(&mut bytes)
        .expect("fixture readable");
    bytes
}

fn dispatch(request: cap_gtfs::Request) -> cap_gtfs::Response {
    cap_gtfs::dispatch(request).expect("dispatch succeeds")
}

/// The module's seam, as a host calls it: the kernel's load_feed on
/// its bulk op, the outcome parsed.
fn load_feed(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_str(&cap_gtfs::invoke_bytes(cap_abi::kernel::OP_LOAD_FEED, bytes)).unwrap()
}

#[test]
fn load_feed_returns_a_content_hash_and_summary() {
    let bytes = fixture_zip();
    let ok = &load_feed(&bytes)["ok"];
    assert_eq!(ok["op"], "load_feed");
    assert_eq!(ok["summary"]["stops"], 6, "A, B, C, the station pin, two bays");
    assert_eq!(ok["summary"]["trips"], 7);
    // The hash is the content hash of the input bytes.
    assert_eq!(ok["feed_hash"], cap_abi::content_hash(&bytes));
}

#[test]
fn loading_the_same_bytes_is_idempotent() {
    let bytes = fixture_zip();
    let first = load_feed(&bytes)["ok"]["feed_hash"].clone();
    let second = load_feed(&bytes)["ok"]["feed_hash"].clone();
    assert_eq!(first, second, "same bytes must land on one registry entry");
}

#[test]
fn unknown_hash_and_bad_input_are_errors_not_panics() {
    let err = cap_gtfs::dispatch(cap_gtfs::Request::FeedInfo {
        feed_hash: "sha256:deadbeef".into(),
    })
    .expect_err("unknown hash must error");
    assert!(err.contains("unknown feed"));

    let err = load_feed(b"not a zip");
    assert!(err["error"].as_str().unwrap().contains("parse"), "garbage must error, not panic: {err}");
}

#[test]
fn json_seam_round_trips_the_tagged_form() {
    let bytes = fixture_zip();
    let request = serde_json::json!({ "op": "load_feed", "bytes": bytes });
    let outcome: serde_json::Value = serde_json::from_str(&cap_gtfs::invoke(&request.to_string())).unwrap();
    assert!(outcome["ok"]["op"] == "load_feed", "tagged response, got {outcome}");
    assert_eq!(outcome["ok"]["feed_hash"], cap_abi::content_hash(&bytes));

    let bad: serde_json::Value = serde_json::from_str(&cap_gtfs::invoke(r#"{"op": "nope"}"#)).unwrap();
    assert!(bad["error"].as_str().unwrap().contains("invalid request"));

    // The retired day ops answer as REQUEST errors — the module no
    // longer speaks them, and the seam says so honestly.
    for retired in [
        "departures",
        "blocks",
        "active_services",
        "block_trips",
        "watchlist_board",
        "stops",
        "stops_by_ids",
    ] {
        let bad: serde_json::Value = serde_json::from_str(&cap_gtfs::invoke(
            &serde_json::json!({ "op": retired, "feed_hash": "sha256:x" }).to_string(),
        ))
        .unwrap();
        assert!(
            bad["error"].as_str().unwrap().contains("invalid request"),
            "{retired} is retired: {bad}"
        );
    }

    // The capability has no bulk op of its own.
    let bad: serde_json::Value = serde_json::from_str(&cap_gtfs::invoke_bytes(9, b"")).unwrap();
    assert!(bad["error"].as_str().unwrap().contains("unknown bulk op"));
}

/// The identity question: which feed is this? The fixture carries
/// feed_info.txt.
#[test]
fn feed_info_answers_the_feeds_identity() {
    let bytes = fixture_zip();
    let feed_hash = load_feed(&bytes)["ok"]["feed_hash"].as_str().unwrap().to_string();
    let info = match dispatch(cap_gtfs::Request::FeedInfo { feed_hash }) {
        cap_gtfs::Response::FeedInfo { info } => info,
        _ => panic!("feed_info"),
    };
    let info = info.expect("the fixture carries feed_info.txt");
    assert_eq!(info.feed_version.as_deref(), Some("2026.10.1"));
    assert_eq!(info.feed_publisher_name.as_deref(), Some("Test Transit"));
}

