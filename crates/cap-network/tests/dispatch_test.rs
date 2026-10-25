//! Native dispatch tests for the network capability — the seam
//! without wasm, over the REAL fixture: the e2e sample feed, the
//! same bytes the corpus and the caps smoke load. The extraction's
//! own proof lives here: the module answers the planner's whole
//! bootstrap → author → publish flow (import, validate, export), and
//! the produced artifact is a first-class feed document (the
//! exported zip parses back to the same identity).

use cap_network::{dispatch, dispatch_json, Request, Response};
use sha2::Digest;

const FIXTURE: &[u8] = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");

/// The shared authored-network fixture — the document the corpus and
/// the monolith's dispatch tests materialize.
const NETWORK_DOC: &str = include_str!("../../../testdata/network.json");

fn load_fixture() -> (String, transit_core::FeedSummary) {
    match dispatch(Request::LoadFeed { bytes: FIXTURE.to_vec() }).unwrap() {
        Response::LoadFeed { feed_hash, summary } => (feed_hash, summary),
        _ => panic!("load_feed"),
    }
}

#[test]
fn load_feed_registers_under_the_content_hash() {
    let (hash, summary) = load_fixture();
    assert_eq!(hash, format!("sha256:{:x}", sha2::Sha256::digest(FIXTURE)));
    assert_eq!(summary.stops, 3);
    // Idempotent: the same bytes load to the same hash.
    let (again, _) = load_fixture();
    assert_eq!(again, hash);
}

#[test]
fn import_infers_and_registers_the_document() {
    let (feed_hash, _) = load_fixture();
    let (network, notes, network_hash) =
        match dispatch(Request::Import { feed_hash }).unwrap() {
            Response::Import { network, notes, network_hash } => (network, notes, network_hash),
            _ => panic!("import"),
        };
    // Valid by construction — the codec's law.
    assert!(network.validate().is_ok(),
        "inferred document must validate: {:?}", transit_network::validation_issues(&network));
    assert_eq!(network.stops.len(), 3);
    assert_eq!(network.patterns.len(), 3);
    // The fixture's single-stop trip is a REPORTED loss, never silent.
    assert!(!notes.is_empty(), "the dropped single-stop trip is reported: {notes:?}");

    // The module is the hash authority: the canonical serialization
    // of the returned document hashes to the returned hash.
    let canonical = serde_json::to_string(&network).unwrap();
    let expect = format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()));
    assert_eq!(network_hash, expect);

    // And the derived document is immediately addressable.
    match dispatch(Request::Validate { network_hash }).unwrap() {
        Response::Validate { issues } => assert!(issues.is_empty()),
        _ => panic!("validate"),
    }
}

#[test]
fn load_document_is_idempotent_by_hash() {
    let doc: transit_network::Network = serde_json::from_str(NETWORK_DOC).unwrap();
    let a = match dispatch(Request::LoadDocument { network: doc.clone() }).unwrap() {
        Response::LoadDocument { network_hash } => network_hash,
        _ => panic!("load_document"),
    };
    let b = match dispatch(Request::LoadDocument { network: doc }).unwrap() {
        Response::LoadDocument { network_hash } => network_hash,
        _ => panic!("load_document"),
    };
    assert_eq!(a, b, "same document, same hash, one registry entry");
    assert!(a.starts_with("sha256:"));
}

#[test]
fn validate_reports_every_issue_without_throwing() {
    let doc: transit_network::Network = serde_json::from_str(NETWORK_DOC).unwrap();
    let network_hash = match dispatch(Request::LoadDocument { network: doc }).unwrap() {
        Response::LoadDocument { network_hash } => network_hash,
        _ => panic!("load_document"),
    };
    match dispatch(Request::Validate { network_hash }).unwrap() {
        Response::Validate { issues } => assert!(issues.is_empty(), "the fixture is valid"),
        _ => panic!("validate"),
    }

    // A referential break: one defect, the whole list, no throw.
    let broken = NETWORK_DOC.replace("\"stop\": \"B\"", "\"stop\": \"ghost\"");
    let doc: transit_network::Network = serde_json::from_str(&broken).unwrap();
    let broken_hash = match dispatch(Request::LoadDocument { network: doc }).unwrap() {
        Response::LoadDocument { network_hash } => network_hash,
        _ => panic!("load_document"),
    };
    match dispatch(Request::Validate { network_hash: broken_hash }).unwrap() {
        Response::Validate { issues } => {
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
    let network_hash = match dispatch(Request::LoadDocument { network: doc }).unwrap() {
        Response::LoadDocument { network_hash } => network_hash,
        _ => panic!("load_document"),
    };
    let (feed_hash, summary, zip_b64) =
        match dispatch(Request::Export { network_hash }).unwrap() {
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
    match dispatch(Request::LoadFeed { bytes: zip }).unwrap() {
        Response::LoadFeed { feed_hash: again, summary: reparsed } => {
            assert_eq!(again, feed_hash, "the exported zip loads to its own advertised hash");
            assert_eq!(
                (reparsed.stops, reparsed.trips, reparsed.stop_times),
                (summary.stops, summary.trips, summary.stop_times),
                "the reparsed summary matches the export's"
            );
        }
        _ => panic!("load_feed"),
    }
}

#[test]
fn export_refuses_invalid_documents() {
    let broken = NETWORK_DOC.replace("\"stop\": \"B\"", "\"stop\": \"ghost\"");
    let doc: transit_network::Network = serde_json::from_str(&broken).unwrap();
    let network_hash = match dispatch(Request::LoadDocument { network: doc }).unwrap() {
        Response::LoadDocument { network_hash } => network_hash,
        _ => panic!("load_document"),
    };
    let err = dispatch(Request::Export { network_hash }).unwrap_err();
    assert!(err.contains("invalid network"), "{err}");
}

#[test]
fn unknown_hashes_are_errors_not_panics() {
    let err = dispatch(Request::Import { feed_hash: "sha256:missing".into() }).unwrap_err();
    assert!(err.contains("unknown feed"), "{err}");
    let err = dispatch(Request::Validate { network_hash: "sha256:missing".into() }).unwrap_err();
    assert!(err.contains("unknown network"), "{err}");
    let err = dispatch(Request::Export { network_hash: "sha256:missing".into() }).unwrap_err();
    assert!(err.contains("unknown network"), "{err}");
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
