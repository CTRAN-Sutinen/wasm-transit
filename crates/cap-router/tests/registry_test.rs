//! Pins the bound on the derived graphs (`cap_abi::kernel::GRAPH`). Linear
//! memory never shrinks, so a long session must not keep a RAPTOR graph for
//! every date. The registries are process-global and eviction would race the
//! dispatch tests, so this file is its own test binary.

use cap_abi::kernel::{kind, derived_len, GRAPH};
use cap_abi::registry::OP_EVICT;
use cap_router::{dispatch, invoke_bytes, Request, Response};

const FIXTURE: &[u8] = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");

fn plan(network_hash: &str, date: &str) -> Result<Response, cap_abi::Refusal> {
    dispatch(Request::Plan {
        network_hash: network_hash.into(),
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: date.into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
        live_hash: None,
        walks_hash: None,
        direct: None,
    })
}

#[test]
fn graphs_are_bounded_and_follow_their_documents_out() {
    let feed = transit_codec::Feed::from_zip_bytes(FIXTURE).unwrap();
    let (network_hash, _) = kind::<transit_network::Network>().register(transit_codec::import(&feed).network).unwrap();

    for day in 1..=30 {
        plan(&network_hash, &format!("2026-10-{day:02}")).unwrap();
        assert!(derived_len(&GRAPH) <= 8, "day {day}: {} graphs held", derived_len(&GRAPH));
    }
    assert_eq!(derived_len(&GRAPH), 8);

    // Evicting with the network on the keep list keeps its graphs. Evicting
    // without it drops them too.
    let keep = serde_json::json!({ "keep": [network_hash] }).to_string();
    invoke_bytes(OP_EVICT, keep.as_bytes());
    assert_eq!(derived_len(&GRAPH), 8);
    invoke_bytes(OP_EVICT, br#"{"keep":[]}"#);
    assert!(derived_len(&GRAPH) == 0 && kind::<transit_network::Network>().is_empty());

    assert_eq!(
        plan(&network_hash, "2026-10-23").unwrap_err().code,
        Some(cap_abi::RefusalCode::UnknownDocument {
            kind: "network".into(),
            hash: network_hash.clone(),
            load: "load_network".into()
        })
    );
}
