//! Native dispatch tests for the network capability — the seam
//! without wasm, over the REAL fixture: the e2e sample feed, the
//! same bytes the caps smoke loads. The module answers the
//! planner's whole
//! bootstrap → author → publish flow (import, validate, export), and
//! the produced artifact is a first-class feed document (the
//! exported zip parses back to the same identity).

use cap_network::{dispatch, invoke as dispatch_json, Request, Response};
use sha2::Digest;

const FIXTURE: &[u8] = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");

/// The shared authored-network fixture — the document the smoke
/// and the manifest's sample load request carry.
const NETWORK_DOC: &str = include_str!("../../../testdata/network.json");

/// The kernel's load_feed, through the module's seam: its hash and summary.
fn load_feed(bytes: &[u8]) -> (String, transit_codec::FeedSummary) {
    let (hash, feed) = cap_abi::kernel::FEED.register_bytes(bytes, None).expect("the feed parses");
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_network::invoke_bytes(cap_abi::kernel::OP_LOAD_FEED, bytes)).unwrap();
    assert_eq!(outcome["ok"]["feed_hash"], hash.as_str(), "the seam answers the kernel's identity");
    (hash, feed.summary())
}

fn load_fixture() -> (String, transit_codec::FeedSummary) {
    load_feed(FIXTURE)
}

#[test]
fn load_feed_registers_under_the_content_hash() {
    let (hash, summary) = load_fixture();
    assert_eq!(hash, format!("sha256:{:x}", sha2::Sha256::digest(FIXTURE)));
    assert_eq!(summary.stops, 6, "A, B, C, the station pin, two bays");
    // Idempotent: the same bytes load to the same hash.
    let (again, _) = load_fixture();
    assert_eq!(again, hash);
}

#[test]
fn import_infers_and_registers_the_document() {
    let (feed_hash, _) = load_fixture();
    let (network, notes, network_hash) =
        match dispatch(Request::Import { feed_hash }).unwrap() {
            Response::Import { network, notes, network_hash, .. } => (network, notes, network_hash),
            _ => panic!("import"),
        };
    // Valid by construction — the codec's law.
    assert!(network.validate().is_ok(),
        "inferred document must validate: {:?}", transit_network::validation_issues(&network));
    assert_eq!(network.stops.len(), 6, "A, B, C, the station pin, two bays");
    assert_eq!(network.patterns.len(), 6, "10's A→B with and without its shape, A→C→B, 20, the shuttle both ways");
    // The fixture's single-stop trip is a REPORTED loss, never silent.
    assert!(!notes.is_empty(), "the dropped single-stop trip is reported: {notes:?}");

    // The module is the hash authority: the canonical serialization
    // of the returned document hashes to the returned hash.
    let canonical = serde_json::to_string(&network).unwrap();
    let expect = format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()));
    assert_eq!(network_hash, expect);

    // And the derived document is immediately addressable.
    match dispatch(Request::Validate { network_hash }).unwrap() {
        Response::Validate { issues, .. } => assert!(issues.is_empty()),
        _ => panic!("validate"),
    }
}

/// The import's boundary registers beside its document, and an export
/// naming it writes the imported numbering back: the published feed
/// re-imports to the same document and the same boundary.
#[test]
fn an_export_naming_the_import_boundary_round_trips() {
    let (feed_hash, _) = load_fixture();
    let Response::Import { network_hash, boundary, boundary_hash, .. } = dispatch(Request::Import { feed_hash }).unwrap()
    else {
        panic!("import")
    };
    assert_eq!(boundary_hash, cap_abi::canonical_hash(&boundary).unwrap(), "the boundary's identity is its canonical hash");
    let Response::Export { zip_b64, .. } =
        dispatch(Request::Export { network_hash: network_hash.clone(), boundary_hash: Some(boundary_hash.clone()) }).unwrap()
    else {
        panic!("export")
    };
    let (again, _) = load_feed(&base64_decode(&zip_b64));
    let Response::Import { network_hash: same, boundary_hash: same_boundary, .. } =
        dispatch(Request::Import { feed_hash: again }).unwrap()
    else {
        panic!("re-import")
    };
    assert_eq!((same, same_boundary), (network_hash, boundary_hash));
}

#[test]
fn load_network_is_idempotent_by_hash() {
    let doc: transit_network::Network = serde_json::from_str(NETWORK_DOC).unwrap();
    let a = cap_abi::kernel::NETWORK.register(doc.clone()).unwrap().0;
    let b = cap_abi::kernel::NETWORK.register(doc).unwrap().0;
    assert_eq!(a, b, "same document, same hash, one registry entry");
    assert!(a.starts_with("sha256:"));
}

#[test]
fn validate_reports_every_issue_without_throwing() {
    let doc: transit_network::Network = serde_json::from_str(NETWORK_DOC).unwrap();
    let network_hash = cap_abi::kernel::NETWORK.register(doc).unwrap().0;
    match dispatch(Request::Validate { network_hash }).unwrap() {
        Response::Validate { issues, .. } => assert!(issues.is_empty(), "the fixture is valid"),
        _ => panic!("validate"),
    }

    // A referential break: one defect, the whole list, no throw.
    let broken = NETWORK_DOC.replace("\"stop\": \"B\"", "\"stop\": \"ghost\"");
    let doc: transit_network::Network = serde_json::from_str(&broken).unwrap();
    let broken_hash = cap_abi::kernel::NETWORK.register(doc).unwrap().0;
    match dispatch(Request::Validate { network_hash: broken_hash }).unwrap() {
        Response::Validate { issues, .. } => {
            assert!(!issues.is_empty(), "the broken document reports its issues");
        }
        _ => panic!("validate"),
    }
}

/// A standard base64 decoder (test-side): the export payload must
/// decode to exactly the bytes whose hash the op returned.
fn base64_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let value = |c: u8| ALPHABET.iter().position(|&a| a == c).unwrap() as u32;
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let n = value(chunk[0]) << 18 | value(chunk[1]) << 12
            | if chunk[2] == b'=' { 0 } else { value(chunk[2]) } << 6
            | if chunk[3] == b'=' { 0 } else { value(chunk[3]) };
        out.push((n >> 16) as u8);
        if chunk[2] != b'=' {
            out.push((n >> 8) as u8);
        }
        if chunk[3] != b'=' {
            out.push(n as u8);
        }
    }
    out
}

#[test]
fn export_is_the_publish_path_and_the_identity_law() {
    let doc: transit_network::Network = serde_json::from_str(NETWORK_DOC).unwrap();
    let network_hash = cap_abi::kernel::NETWORK.register(doc).unwrap().0;
    let (feed_hash, summary, zip_b64) =
        match dispatch(Request::Export { network_hash, boundary_hash: None }).unwrap() {
            Response::Export { feed_hash, summary, zip_b64 } => (feed_hash, summary, zip_b64),
            _ => panic!("export"),
        };
    assert_eq!(summary.stops, 3);
    assert_eq!(summary.trips, 3);

    // The hash authority: the returned hash is the content hash of
    // the decoded payload — the same identity any module would
    // return for the same bytes.
    let zip = base64_decode(&zip_b64);
    assert_eq!(feed_hash, format!("sha256:{:x}", sha2::Sha256::digest(&zip)));

    // The exported artifact is a first-class feed: loading the bytes
    // back lands on the SAME hash (the composition moment — the
    // handoff a host asserts).
    let (again, reparsed) = load_feed(&zip);
    assert_eq!(again, feed_hash, "the exported zip loads to its own advertised hash");
    assert_eq!(
        (reparsed.stops, reparsed.trips, reparsed.stop_times),
        (summary.stops, summary.trips, summary.stop_times),
        "the reparsed summary matches the export's"
    );
}

#[test]
fn export_refuses_invalid_documents() {
    let broken = NETWORK_DOC.replace("\"stop\": \"B\"", "\"stop\": \"ghost\"");
    let doc: transit_network::Network = serde_json::from_str(&broken).unwrap();
    let network_hash = cap_abi::kernel::NETWORK.register(doc).unwrap().0;
    let err = dispatch(Request::Export { network_hash, boundary_hash: None }).unwrap_err();
    assert!(err.contains("invalid network"), "{err}");
}

#[test]
fn unknown_hashes_are_errors_not_panics() {
    let err = dispatch(Request::Import { feed_hash: "sha256:missing".into() }).unwrap_err();
    assert!(err.contains("unknown feed"), "{err}");
    let err = dispatch(Request::Validate { network_hash: "sha256:missing".into() }).unwrap_err();
    assert!(err.contains("unknown network"), "{err}");
    let err = dispatch(Request::Export { network_hash: "sha256:missing".into(), boundary_hash: None }).unwrap_err();
    assert!(err.contains("unknown network"), "{err}");
    let (feed_hash, _) = load_fixture();
    let Response::Import { network_hash, .. } = dispatch(Request::Import { feed_hash }).unwrap() else { panic!("import") };
    let err = dispatch(Request::Export { network_hash, boundary_hash: Some("sha256:missing".into()) }).unwrap_err();
    assert!(err.contains("unknown boundary"), "{err}");
}

#[test]
fn json_seam_round_trips_the_tagged_form() {
    let load = dispatch_json(&format!(
        "{{\"op\":\"load_feed\",\"bytes\":{}}}",
        serde_json::to_string(&FIXTURE.to_vec()).unwrap()
    ));
    let parsed: serde_json::Value = serde_json::from_str(&load).unwrap();
    assert!(parsed.get("ok").is_some(), "{load}");
    let feed_hash = parsed["ok"]["feed_hash"].as_str().unwrap().to_string();

    let outcome = dispatch_json(&format!(
        r#"{{"op":"import","feed_hash":"{feed_hash}"}}"#
    ));
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    assert!(parsed["ok"]["op"] == "import", "{outcome}");
    assert!(parsed["ok"]["network_hash"].as_str().unwrap().starts_with("sha256:"));

    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_json(r#"{"op":"import","feed_hash":"sha256:missing"}"#);
    assert!(bad.contains("unknown feed"), "{bad}");
}

#[test]
fn the_kernel_bulk_op_registers_the_same_identity_as_load_network() {
    // The bare document on the kernel's bulk op — parsed straight into
    // the model, no request envelope — answers exactly what the JSON
    // op answers; garbage bytes are an error outcome, never a trap.
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_network::invoke(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_network::invoke_bytes(cap_abi::OP_LOAD_NETWORK, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_network::invoke_bytes(cap_abi::OP_LOAD_NETWORK, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}

/// The publish path's identity: a host that stores a network (the
/// server's `plan.<id>` pointer) stores the module's canonical bytes
/// under the module's hash — never the bytes it was sent, which may
/// be any serialization of the same document (a browser's
/// JSON.stringify writes 1.0 as 1). The answer carries the plan frame
/// so the host never reads the document.
#[test]
fn document_answers_the_canonical_bytes_the_hash_names() {
    let doc: transit_network::Network = serde_json::from_str(NETWORK_DOC).unwrap();
    // Any serialization of the same document: pretty-printed here.
    let pretty = serde_json::to_string_pretty(&doc).unwrap();
    let loaded: serde_json::Value =
        serde_json::from_str(&cap_network::invoke_bytes(cap_abi::OP_LOAD_NETWORK, pretty.as_bytes())).unwrap();
    let network_hash = loaded["ok"]["network_hash"].as_str().expect("load_network answers").to_string();
    match dispatch(Request::Document { network_hash: network_hash.clone() }).unwrap() {
        Response::Document { document, document_hash, plan } => {
            assert_eq!(document_hash, network_hash, "the document hash IS the network hash");
            assert_eq!(cap_abi::content_hash(document.as_bytes()), network_hash, "the bytes are the ones the hash names");
            assert_ne!(document, pretty, "canonical bytes, not the bytes it was sent");
            assert_eq!(plan, doc.plan, "the plan frame rides along");
        }
        other => panic!("document: {other:?}"),
    }
    assert!(dispatch(Request::Document { network_hash: "sha256:00".into() }).is_err());
}

/// What stops a network from being published, answered by the module
/// (D-76): its validation issues, and the warnings that refuse
/// publication — a pick that runs no service in its period. The other
/// warnings ride along and never refuse.
#[test]
fn publishable_answers_what_refuses_a_pick() {
    let mut doc: transit_network::Network = serde_json::from_str(NETWORK_DOC).unwrap();
    let start = doc.service_types.values().flat_map(|st| st.rule.ranges.iter().map(|r| r.start)).min().unwrap();
    doc.plan = transit_network::PlanMeta { id: "fall".into(), name: "Fall".into(), effective_from: Some(start) };
    let in_calendar = cap_abi::kernel::NETWORK.register(doc.clone()).unwrap().0;
    match dispatch(Request::Publishable { network_hash: in_calendar }).unwrap() {
        Response::Publishable { issues, refusals, .. } => {
            assert!(issues.is_empty() && refusals.is_empty(), "{issues:?} {refusals:?}")
        }
        other => panic!("publishable: {other:?}"),
    }
    doc.plan.effective_from = chrono::NaiveDate::from_ymd_opt(2099, 1, 5);
    let past_calendar = cap_abi::kernel::NETWORK.register(doc).unwrap().0;
    match dispatch(Request::Publishable { network_hash: past_calendar }).unwrap() {
        Response::Publishable { refusals, .. } => assert!(
            refusals.iter().any(|w| matches!(w, transit_network::NetworkWarning::NoServiceInPlanPeriod { .. })),
            "{refusals:?}"
        ),
        other => panic!("publishable: {other:?}"),
    }
}
