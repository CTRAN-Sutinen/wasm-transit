//! Native dispatch tests for the gtfs capability — the seam without
//! wasm (the same law the monolith's dispatch_test holds). The
//! content-hash registry gets explicit laws: same bytes, same hash,
//! one entry (idempotence); different bytes, different hashes; and
//! the registry is addressed by hash, so a query for an unknown hash
//! is an error, never a panic.

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

#[test]
fn load_feed_returns_a_content_hash_and_summary() {
    let bytes = fixture_zip();
    let cap_gtfs::Response::LoadFeed { feed_hash, summary } =
        dispatch(cap_gtfs::Request::LoadFeed { bytes: bytes.clone() })
    else {
        panic!("load_feed response");
    };
    assert!(feed_hash.starts_with("sha256:"));
    assert_eq!(summary.stops, 3);
    assert_eq!(summary.trips, 5);

    // The hash is the content hash of the input bytes.
    assert_eq!(feed_hash, cap_gtfs::content_hash(&bytes));
}

#[test]
fn loading_the_same_bytes_is_idempotent() {
    let bytes = fixture_zip();
    let cap_gtfs::Response::LoadFeed { feed_hash: first, .. } =
        dispatch(cap_gtfs::Request::LoadFeed { bytes: bytes.clone() })
    else {
        panic!("first load")
    };
    let cap_gtfs::Response::LoadFeed { feed_hash: second, .. } =
        dispatch(cap_gtfs::Request::LoadFeed { bytes })
    else {
        panic!("second load")
    };
    assert_eq!(first, second, "same bytes must land on one registry entry");
}

#[test]
fn departures_answer_by_hash_on_the_fixture_date() {
    let bytes = fixture_zip();
    let feed_hash = match dispatch(cap_gtfs::Request::LoadFeed { bytes }) {
        cap_gtfs::Response::LoadFeed { feed_hash, .. } => feed_hash,
        _ => panic!("load"),
    };
    // Friday 2026-10-23: WK plus the HOL addition — the fixture's
    // loaded date.
    let cap_gtfs::Response::Departures { departures } = dispatch(cap_gtfs::Request::Departures {
        feed_hash: feed_hash.clone(),
        stop_id: "A".into(),
        date: "2026-10-23".into(),
        from_secs: 7 * 3600,
        limit: 10,
    })
    else {
        panic!("departures response");
    };
    let times: Vec<u32> = departures.iter().map(|d| d.departure_time.unwrap()).collect();
    assert_eq!(times, vec![8 * 3600, 8 * 3600 + 20 * 60, 8 * 3600 + 40 * 60]);
    let routes: Vec<&str> = departures.iter().map(|d| d.route_name.as_str()).collect();
    assert_eq!(routes, vec!["10", "20", "10"]);
}

#[test]
fn stops_lists_by_hash() {
    let bytes = fixture_zip();
    let feed_hash = match dispatch(cap_gtfs::Request::LoadFeed { bytes }) {
        cap_gtfs::Response::LoadFeed { feed_hash, .. } => feed_hash,
        _ => panic!("load"),
    };
    let cap_gtfs::Response::Stops { stops } = dispatch(cap_gtfs::Request::Stops {
        feed_hash,
        limit: 2,
    })
    else {
        panic!("stops response");
    };
    assert_eq!(stops.len(), 2);
    assert_eq!(stops[0].stop_id, "A");
}

#[test]
fn unknown_hash_and_bad_input_are_errors_not_panics() {
    let err = cap_gtfs::dispatch(cap_gtfs::Request::Departures {
        feed_hash: "sha256:deadbeef".into(),
        stop_id: "A".into(),
        date: "2026-10-23".into(),
        from_secs: 0,
        limit: 5,
    })
    .expect_err("unknown hash must error");
    assert!(err.contains("unknown feed"));

    let err = cap_gtfs::dispatch(cap_gtfs::Request::LoadFeed { bytes: b"not a zip".to_vec() })
        .expect_err("garbage must error, not panic");
    assert!(err.contains("parse"));
}

#[test]
fn json_seam_round_trips_the_tagged_form() {
    let bytes = fixture_zip();
    let request = serde_json::json!({ "op": "load_feed", "bytes": bytes });
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_gtfs::dispatch_json(&request.to_string())).unwrap();
    assert!(outcome["ok"]["op"] == "load_feed", "tagged response, got {outcome}");
    assert!(outcome["ok"]["feed_hash"].as_str().unwrap().starts_with("sha256:"));

    let bad: serde_json::Value =
        serde_json::from_str(&cap_gtfs::dispatch_json(r#"{"op": "nope"}"#)).unwrap();
    assert!(bad["error"].as_str().unwrap().contains("invalid request"));

    // The bulk-bytes seam loads a feed zip without JSON-encoding it.
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_gtfs::dispatch_bytes(0, &fixture_zip())).unwrap();
    assert!(outcome["ok"]["feed_hash"].as_str().unwrap().starts_with("sha256:"));
    let bad: serde_json::Value =
        serde_json::from_str(&cap_gtfs::dispatch_bytes(9, b"")).unwrap();
    assert!(bad["error"].as_str().unwrap().contains("unknown bulk op"));
}
