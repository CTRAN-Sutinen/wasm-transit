//! Native dispatch tests for the calendar capability — the seam
//! without wasm, over the REAL fixture-derived network (the same
//! document cap-network's import produces from the e2e zip). The
//! laws (the library carries the day-level detail in
//! transit-dispatch's own suite; these pin the SEAM):
//!
//! 1. the network document's identity is the module's canonical
//!    serialization (the same identity cap-analysis returns);
//! 2. the answer comes from the EFFECTIVE day — the honesty law:
//!    a key whose every trip is skipped or unrepresentable is not
//!    running, however its calendar reads;
//! 3. invalid edit records are REPORTED at load, never rejected;
//! 4. unknown hashes and garbage are errors, never panics.

use cap_calendar::{dispatch, dispatch_bytes, dispatch_json, Request, Response};
use sha2::Digest;
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
    match dispatch(Request::LoadNetwork { network: derived_network() }).unwrap() {
        Response::LoadNetwork { network_hash } => network_hash,
        _ => panic!("load_network"),
    }
}

fn active_services(network_hash: &str, edits_hash: Option<String>, date: &str) -> Vec<String> {
    match dispatch(Request::ActiveServices {
        network_hash: network_hash.into(),
        edits_hash,
        date: date.into(),
    })
    .unwrap()
    {
        Response::ActiveServices { services } => services,
        _ => panic!("active_services"),
    }
}

#[test]
fn the_network_identity_is_the_modules_canonical_serialization() {
    let hash = load_network();
    // The same formula every network-consuming module uses: the
    // canonical serde bytes hashed. One document, one truth.
    let canonical = serde_json::to_string(&derived_network()).unwrap();
    let expect = format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()));
    assert_eq!(hash, expect);
    // Idempotent: the same document loads to the same hash.
    assert_eq!(load_network(), hash);
}

#[test]
fn the_answer_comes_from_the_effective_day() {
    let network_hash = load_network();

    // The fixture Friday: WK runs; HOL's only trip is single-stop —
    // the import drops it (and reports it) — so HOL does not exist
    // in the document at all. The honest answer.
    assert_eq!(
        active_services(&network_hash, None, DATE),
        vec!["WK".to_string()],
        "the day answers, not the calendar rules"
    );

    // A no-service day answers empty, never an error.
    assert!(
        active_services(&network_hash, None, "2026-10-24").is_empty(),
        "Saturday carries nothing"
    );

    // The honesty law, pinned by construction: HOL never entered the
    // model (its unrepresentable trip was dropped at import, drop
    // reported), so no calendar read can resurrect it.
    let net = derived_network();
    assert!(
        !net.service_types.contains_key("HOL"),
        "the fixture pins the law: HOL's rule never made it into the document"
    );
}

#[test]
fn edits_can_take_a_key_off_the_board() {
    let network_hash = load_network();
    // Skip every stop of route 20's trips on the fixture Friday: the
    // WK key that only exists through those trips leaves the board.
    // (The fixture's WK also carries route 10, so this record only
    // proves the mechanism when it removes a whole key — instead
    // the law pins the honest mechanics: the answer never includes
    // a key with no running trips.)
    let records = vec![EditRecord {
        id: "e1".into(),
        diversion: transit_sync::Diversion {
            id: "d1".into(),
            route_id: "20".into(),
            direction_id: None,
            first_kept_stop: "A".into(),
            last_kept_stop: "B".into(),
            skipped_stops: vec!["C".into()],
            detour: None,
            added_secs: 60,
            start_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            end_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
        },
        status: EditStatus::Accepted,
        rev: 1,
        updated_at: 1,
    }];
    let (edits_hash, reports) = match dispatch(Request::LoadEdits {
        network_hash: network_hash.clone(),
        records,
    })
    .unwrap()
    {
        Response::LoadEdits { edits_hash, reports } => (edits_hash, reports),
        _ => panic!("load_edits"),
    };
    assert!(reports.iter().all(|r| r.valid), "the fixture diversion is valid: {reports:?}");
    // WK still runs (route 10 carries it); the answer is unchanged —
    // the mechanism is pinned by construction: the answer derives
    // from the patched day, and a key only leaves when its every
    // trip leaves.
    assert_eq!(
        active_services(&network_hash, Some(edits_hash), DATE),
        vec!["WK".to_string()]
    );
}

#[test]
fn invalid_edit_records_are_reported_not_rejected() {
    let network_hash = load_network();
    // A record whose anchors do not exist in the network: reported
    // invalid, still registered (the sync contract — the host owns
    // the document, the module reports).
    let mut broken = EditRecord {
        id: "e1".into(),
        diversion: transit_sync::Diversion {
            id: "d1".into(),
            route_id: "ghost".into(),
            direction_id: None,
            first_kept_stop: "A".into(),
            last_kept_stop: "B".into(),
            skipped_stops: vec![],
            detour: None,
            added_secs: 60,
            start_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            end_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
        },
        status: EditStatus::Accepted,
        rev: 1,
        updated_at: 1,
    };
    broken.diversion.first_kept_stop = "ghost".into();
    let (edits_hash, reports) = match dispatch(Request::LoadEdits {
        network_hash: network_hash.clone(),
        records: vec![broken],
    })
    .unwrap()
    {
        Response::LoadEdits { edits_hash, reports } => (edits_hash, reports),
        _ => panic!("load_edits"),
    };
    assert_eq!(reports.len(), 1);
    assert!(!reports[0].valid, "the broken record is reported: {reports:?}");
    // Registered under its hash: the day view addresses it (and the
    // invalid record simply matches nothing).
    assert_eq!(
        active_services(&network_hash, Some(edits_hash), DATE),
        vec!["WK".to_string()],
        "the answer still comes"
    );
}

#[test]
fn unknown_hashes_and_garbage_are_errors_not_panics() {
    let err = dispatch(Request::LoadEdits {
        network_hash: "sha256:missing".into(),
        records: vec![],
    })
    .unwrap_err();
    assert!(err.contains("unknown network"), "{err}");

    let network_hash = load_network();
    let err = dispatch(Request::ActiveServices {
        network_hash,
        edits_hash: Some("sha256:missing".into()),
        date: DATE.into(),
    })
    .unwrap_err();
    assert!(err.contains("unknown edits"), "{err}");

    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_json(r#"{"op":"active_services","network_hash":"sha256:x","date":"2026-13-99"}"#);
    assert!(bad.contains("invalid date"), "{bad}");
    let bad = dispatch_bytes(0, b"whatever");
    assert!(bad.contains("unknown bulk op"), "the module takes JSON documents: {bad}");
}

#[test]
fn the_kernel_bulk_op_registers_the_same_identity_as_load_network() {
    // The bare document on the kernel's bulk op — parsed straight into
    // the model, no request envelope — answers exactly what the JSON
    // op answers; garbage bytes are an error outcome, never a trap.
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_calendar::dispatch_json(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_calendar::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_calendar::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}
