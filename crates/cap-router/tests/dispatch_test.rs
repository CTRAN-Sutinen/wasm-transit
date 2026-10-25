//! Native dispatch tests for the router capability — the pure-function
//! half of the capability composition. The core law mirrors the
//! monolith's dispatch test with the seam's own vocabulary: the feed
//! is addressed by the SAME content hash the gtfs capability returns
//! for the same bytes — the two modules agree on document identity
//! without sharing anything.

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

fn load() -> String {
    match cap_router::dispatch(cap_router::Request::LoadFeed { bytes: fixture_zip() }) {
        Ok(cap_router::Response::LoadFeed { feed_hash, .. }) => feed_hash,
        _ => panic!("load"),
    }
}

#[test]
fn the_feed_hash_is_the_content_hash_the_whole_system_agrees_on() {
    // The composition law: documents are addressed by content hash,
    // so the host can load the same bytes into any set of capability
    // modules and assert they agree on the identity. The format is
    // "sha256:<hex>" of the input bytes — verified here against the
    // hash computed independently (the same formula cap-gtfs, the
    // manifest generator, and JS crypto.subtle use).
    let bytes = fixture_zip();
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(&bytes);
    let expected = format!("sha256:{:x}", digest);
    match cap_router::dispatch(cap_router::Request::LoadFeed { bytes }) {
        Ok(cap_router::Response::LoadFeed { feed_hash, .. }) => assert_eq!(feed_hash, expected),
        _ => panic!("router load"),
    }
}

#[test]
fn plan_finds_the_earliest_arrival_on_the_fixture() {
    let hash = load();
    let plan = cap_router::dispatch(cap_router::Request::Plan {
        feed_hash: hash,
        from: "A".into(),
        to: "B".into(),
        date: "2026-10-23".into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
    })
    .expect("plan dispatch");
    let cap_router::Response::Plan { itinerary: Some(it) } = plan else {
        panic!("A to B is reachable on the fixture date")
    };
    assert_eq!(it.arrival_secs, 8 * 3600 + 10 * 60, "t10: depart 08:00, arrive 08:10");
}

#[test]
fn plan_is_none_on_an_unreachable_pair_or_day() {
    let hash = load();
    // Sunday: only the SUN service runs (t11 at 09:00). A departure
    // after the last trip is unreachable.
    let late = cap_router::dispatch(cap_router::Request::Plan {
        feed_hash: hash.clone(),
        from: "A".into(),
        to: "B".into(),
        date: "2026-10-25".into(), // Sunday
        depart_secs: 10 * 3600,
        max_legs: 4,
    })
    .unwrap();
    let cap_router::Response::Plan { itinerary } = late else { panic!("plan") };
    assert!(itinerary.is_none(), "no service after 10:00 on Sunday");

    // Unknown hash: an error, not a panic.
    let err = cap_router::dispatch(cap_router::Request::Plan {
        feed_hash: "sha256:missing".into(),
        from: "A".into(),
        to: "B".into(),
        date: "2026-10-23".into(),
        depart_secs: 0,
        max_legs: 4,
    })
    .expect_err("unknown hash");
    assert!(err.contains("unknown feed"));
}

#[test]
fn json_seam_round_trips_the_tagged_form() {
    let hash = load();
    let request = serde_json::json!({
        "op": "plan",
        "feed_hash": hash,
        "from": "A",
        "to": "B",
        "date": "2026-10-23",
        "depart_secs": 25200,
        "max_legs": 4,
    });
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_router::dispatch_json(&request.to_string())).unwrap();
    assert!(outcome["ok"]["op"] == "plan");
    assert_eq!(outcome["ok"]["itinerary"]["arrival_secs"], 29400);

    let outcome: serde_json::Value =
        serde_json::from_str(&cap_router::dispatch_bytes(0, &fixture_zip())).unwrap();
    assert!(outcome["ok"]["feed_hash"].as_str().unwrap().starts_with("sha256:"));
}
