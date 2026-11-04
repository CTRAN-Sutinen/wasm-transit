//! The rt registry is the eviction pin case (QAF-49): an agency
//! re-mints its snapshots every poll, so a session must plateau with
//! no host steering, and a host's keep-set must bound the durable
//! kinds. Its own test binary: the registries are process-global, and
//! eviction would race the dispatch tests' registrations.

use cap_abi::registry::{kernel, OP_EVICT, OP_HELD};
use cap_rt::{dispatch, dispatch_bytes, Request, Response, OP_DECODE, REGISTRIES, SNAPSHOTS};
use prost::Message;
use transit_rt::gtfs_rt as rt;

/// A distinct snapshot per poll: the same feed at another timestamp.
fn snapshot(poll: u64) -> Vec<u8> {
    rt::FeedMessage {
        header: rt::FeedHeader {
            gtfs_realtime_version: "2.0".into(),
            timestamp: Some(1_796_000_000 + poll * 30),
            ..Default::default()
        },
        entity: vec![],
    }
    .encode_to_vec()
}

fn decoded_hash(bytes: &[u8]) -> String {
    let answer: serde_json::Value = serde_json::from_str(&dispatch_bytes(OP_DECODE, bytes)).unwrap();
    answer["ok"]["rt_hash"].as_str().unwrap_or_else(|| panic!("{answer}")).to_string()
}

fn held(answer: &str) -> Vec<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(answer).unwrap();
    value["ok"]["held"]
        .as_array()
        .unwrap_or_else(|| panic!("{value}"))
        .iter()
        .map(|e| (e["kind"].as_str().unwrap().to_string(), e["key"].as_str().unwrap().to_string()))
        .collect()
}

#[test]
fn the_rt_registry_plateaus_and_a_keep_set_bounds_the_rest() {
    // A poll loop: hundreds of snapshots, never more than eight held,
    // and the newest always among them.
    let mut last = String::new();
    for poll in 0..300 {
        last = decoded_hash(&snapshot(poll));
        assert!(SNAPSHOTS.len() <= 8, "poll {poll}: {} snapshots held", SNAPSHOTS.len());
    }
    assert!(SNAPSHOTS.get(&last).is_some(), "the newest snapshot is held");

    // A durable kind stays until the host names its keep-set.
    let feed = transit_codec::Feed::from_zip_bytes(include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    let imported = transit_codec::import(&feed);
    let doc = serde_json::to_vec(&imported.network).unwrap();
    let loaded: serde_json::Value = serde_json::from_str(&dispatch_bytes(cap_abi::OP_LOAD_NETWORK, &doc)).unwrap();
    let network_hash = loaded["ok"]["network_hash"].as_str().unwrap().to_string();
    let Response::LoadBoundary { boundary_hash } = dispatch(Request::LoadBoundary { boundary: imported.boundary }).unwrap() else {
        panic!("load_boundary")
    };
    let before = held(&kernel(OP_HELD, b"", REGISTRIES).unwrap());
    assert!(before.contains(&("network".into(), network_hash.clone())));
    assert!(before.contains(&("boundary".into(), boundary_hash.clone())));

    // The keep-set names the network and the newest snapshot: the
    // boundary and every older snapshot drop, the kept never do.
    let keep = serde_json::json!({ "keep": [network_hash, last] }).to_string();
    let after = held(&dispatch_bytes_kernel(OP_EVICT, keep.as_bytes()));
    assert_eq!(after, vec![("network".into(), network_hash.clone()), ("rt snapshot".into(), last.clone())]);

    // Evicted-then-asked is the honest unknown-hash error, never a
    // wrong answer — and registering again answers the same hash.
    let asked = dispatch(Request::Live {
        network_hash: network_hash.clone(),
        boundary_hash: Some(boundary_hash.clone()),
        rt_hash: Some(last.clone()),
        vp_hash: None,
        alerts_hash: None,
        window: None,
        late_limit: None,
    });
    assert_eq!(asked.unwrap_err(), format!("unknown boundary {boundary_hash}: load_boundary first"));
    let Response::LoadBoundary { boundary_hash: again } =
        dispatch(Request::LoadBoundary { boundary: transit_codec::import(&feed).boundary }).unwrap()
    else {
        panic!("load_boundary")
    };
    assert_eq!(again, boundary_hash);
}

/// The kernel op as a host reaches it (the macro's
/// `transit_invoke_bytes` answers it before the module's dispatch).
fn dispatch_bytes_kernel(op: u32, bytes: &[u8]) -> String {
    kernel(op, bytes, REGISTRIES).expect("a kernel op")
}
