//! Native dispatch tests for the router capability — the SEAM over
//! the NETWORK document and the EFFECTIVE DAY. The laws (the
//! algorithm carries its own suite in transit-router; these pin the
//! seam):
//!
//! 1. the network document's identity is the module's canonical
//!    serialization — the same identity every day-view capability
//!    returns for the same document;
//! 2. a plan is the materialized day (model trips, model
//!    vocabulary), answered by hash pair + date;
//! 3. the compare facet is the diversion delta: the patched day
//!    against the unpatched one — one materialization apart, and no
//!    baseline when no edit is in force;
//! 4. unknown hashes and garbage are errors, never panics.

use transit_network::Network;
use transit_sync::{EditRecord, EditStatus};

const FIXTURE: &[u8] = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");
const DATE: &str = "2026-10-23"; // the fixture's loaded Friday

/// The fixture-derived network document (import is the codec — the
/// boundary; this module composes over the document it produces).
fn derived_network() -> Network {
    let gtfs = transit_core::Gtfs::from_zip_bytes(FIXTURE).expect("fixture parses");
    transit_codec::gtfs_to_network(&gtfs).network
}

fn load_network() -> String {
    match cap_router::dispatch(cap_router::Request::LoadNetwork { network: derived_network() }) {
        Ok(cap_router::Response::LoadNetwork { network_hash }) => network_hash,
        _ => panic!("load_network"),
    }
}

/// The fixture's diversion: route 10 keeps A→B, skips C, +N s.
fn record(added_secs: u32) -> EditRecord {
    EditRecord {
        id: "e1".into(),
        diversion: transit_sync::Diversion {
            id: "d1".into(),
            route_id: "10".into(),
            direction_id: None,
            first_kept_stop: "A".into(),
            last_kept_stop: "B".into(),
            skipped_stops: vec!["C".into()],
            detour: None,
            added_secs,
            start_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            end_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
        },
        status: EditStatus::Accepted,
        rev: 1,
        updated_at: 1,
    }
}

fn load_edits(network_hash: &str, records: Vec<EditRecord>) -> String {
    match cap_router::dispatch(cap_router::Request::LoadEdits {
        network_hash: network_hash.into(),
        records,
    })
    .unwrap()
    {
        cap_router::Response::LoadEdits { edits_hash, .. } => edits_hash,
        _ => panic!("load_edits"),
    }
}

#[test]
fn the_document_identity_is_the_canonical_hash_the_bus_agrees_on() {
    // The composition law: documents are addressed by the hash of
    // the module's canonical serialization, so the host can push the
    // same document into every capability that needs it and assert
    // they agree. Verified against the hash computed independently
    // (the same formula every capability and the manifest use).
    let network = derived_network();
    let canonical = serde_json::to_string(&network).unwrap();
    use sha2::Digest as _;
    let expected = format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()));
    match cap_router::dispatch(cap_router::Request::LoadNetwork { network }) {
        Ok(cap_router::Response::LoadNetwork { network_hash }) => assert_eq!(network_hash, expected),
        _ => panic!("router load_network"),
    }
}

#[test]
fn plan_answers_over_the_materialized_day() {
    let hash = load_network();
    let plan = cap_router::dispatch(cap_router::Request::Plan {
        network_hash: hash,
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
    })
    .expect("plan dispatch");
    let cap_router::Response::Plan { itinerary: Some(it) } = plan else {
        panic!("A to B is reachable on the fixture date")
    };
    assert_eq!(it.arrival_secs, 8 * 3600 + 10 * 60, "t10: depart 08:00, arrive 08:10");
    // The leg's trip identity is the derived key — (schedule,
    // start_secs) — the identity every day view and the codec's
    // trip-id map agree on.
    match &it.legs[0] {
        transit_router::Leg::Transit { pattern_trips, start_secs, .. } => {
            assert!(!pattern_trips.is_empty());
            assert_eq!(*start_secs, 8 * 3600);
        }
        other => panic!("expected a transit leg, got {other:?}"),
    }
}

#[test]
fn plan_is_none_on_an_unreachable_pair_or_day() {
    let hash = load_network();
    // Sunday: only the SUN service runs (t11 at 09:00). A departure
    // after the last trip is unreachable.
    let late = cap_router::dispatch(cap_router::Request::Plan {
        network_hash: hash.clone(),
        edits_hash: None,
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
        network_hash: "sha256:missing".into(),
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 0,
        max_legs: 4,
    })
    .expect_err("unknown hash");
    assert!(err.contains("unknown network"));
}

#[test]
fn compare_answers_the_diversion_delta() {
    let hash = load_network();
    let edits = load_edits(&hash, vec![record(300)]);

    let cmp = cap_router::dispatch(cap_router::Request::PlanCompare {
        network_hash: hash.clone(),
        edits_hash: Some(edits),
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
    })
    .expect("compare dispatch");
    let cap_router::Response::PlanCompare { active, baseline, notice } = cmp else {
        panic!("compare")
    };
    assert_eq!(notice, None, "a found trip needs no notice");
    // The diversion adds 300 s to route 10's kept segment: the
    // patched day arrives 08:15; the unpatched one 08:10.
    assert_eq!(active.expect("active plan").arrival_secs, 8 * 3600 + 15 * 60);
    assert_eq!(baseline.expect("baseline world").arrival_secs, 8 * 3600 + 10 * 60);

    // No edit document: there is no second world to compare
    // against — the baseline is honestly absent.
    let plain = cap_router::dispatch(cap_router::Request::PlanCompare {
        network_hash: hash,
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
    })
    .expect("compare dispatch");
    let cap_router::Response::PlanCompare { active, baseline, .. } = plain else {
        panic!("compare")
    };
    assert!(active.is_some(), "the plan itself still answers");
    assert!(baseline.is_none(), "no edits in force, no baseline");

    // Too late for any trip: the notice says so, legs named.
    let late = cap_router::dispatch(cap_router::Request::PlanCompare {
        network_hash: load_network(),
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 40 * 3600,
        max_legs: 4,
    })
    .expect("compare dispatch");
    let cap_router::Response::PlanCompare { active: None, notice: Some(notice), .. } = late else {
        panic!("an unreachable plan carries its notice")
    };
    assert!(notice.contains("within 4 legs"), "{notice}");
}

#[test]
fn json_seam_round_trips_the_tagged_form() {
    let hash = load_network();
    let request = serde_json::json!({
        "op": "plan",
        "network_hash": hash,
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

    // The router takes JSON documents; a bulk-bytes call is an
    // honest error, never a silent ignore.
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_router::dispatch_bytes(0, b"{}")).unwrap();
    assert!(outcome["error"].as_str().unwrap().contains("unknown bulk op"));
}

#[test]
fn an_itinerary_draws_its_ridden_segments() {
    let hash = load_network();
    let cap_router::Response::Plan { itinerary: Some(it) } = cap_router::dispatch(cap_router::Request::Plan {
        network_hash: hash.clone(),
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
    })
    .unwrap() else {
        panic!("plan")
    };
    let legs = it.legs.len();
    let cap_router::Response::ItineraryPaths { paths } = cap_router::dispatch(cap_router::Request::ItineraryPaths {
        network_hash: hash,
        edits_hash: None,
        date: DATE.into(),
        itinerary: it,
    })
    .unwrap() else {
        panic!("itinerary_paths")
    };
    assert_eq!(paths.len(), legs, "one path per leg");
    assert_eq!(paths[0].kind, transit_dispatch::LegKind::Transit);
    assert!(paths[0].path.len() >= 2, "the ridden segment is a line");
}

#[test]
fn the_kernel_bulk_op_registers_the_same_identity_as_load_network() {
    // The bare document on the kernel's bulk op — parsed straight into
    // the model, no request envelope — answers exactly what the JSON
    // op answers; garbage bytes are an error outcome, never a trap.
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_router::dispatch_json(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_router::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_router::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}
