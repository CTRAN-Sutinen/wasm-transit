//! The router's graphs are derived entries (`cap_abi::kernel::GRAPH`):
//! bounded on their own (a long session's dates would each leave a
//! RAPTOR graph behind in linear memory that never shrinks), and
//! dropped with any document they were computed from. Its own test
//! binary: the registries are process-global, and eviction would race
//! the dispatch tests.

use cap_abi::kernel::{derived_len, GRAPH, NETWORK};
use cap_abi::registry::OP_EVICT;
use cap_router::{dispatch, invoke_bytes, Request, Response};

const FIXTURE: &[u8] = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");

fn plan(network_hash: &str, date: &str) -> Result<Response, String> {
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
    let (network_hash, _) = NETWORK.register(transit_codec::import(&feed).network).unwrap();

    // A session that asks across a month of dates: one graph a day,
    // never more than eight held.
    for day in 1..=30 {
        plan(&network_hash, &format!("2026-10-{day:02}")).unwrap();
        assert!(derived_len(&GRAPH) <= 8, "day {day}: {} graphs held", derived_len(&GRAPH));
    }
    assert_eq!(derived_len(&GRAPH), 8);

    // A keep-set that keeps the network keeps its graphs; one that
    // drops it drops them with it.
    let keep = serde_json::json!({ "keep": [network_hash] }).to_string();
    invoke_bytes(OP_EVICT, keep.as_bytes());
    assert_eq!(derived_len(&GRAPH), 8);
    invoke_bytes(OP_EVICT, br#"{"keep":[]}"#);
    assert!(derived_len(&GRAPH) == 0 && NETWORK.is_empty());

    // Evicted-then-asked is the unknown-hash error, never a stale graph.
    assert_eq!(
        plan(&network_hash, "2026-10-23").unwrap_err(),
        format!("unknown network {network_hash}: load_network first")
    );
}
