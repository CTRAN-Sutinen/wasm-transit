//! Native dispatch tests for the street-graph capability — the seam
//! without wasm. The Elbow route must follow street geometry (the
//! corner, not the diagonal), and the seam must be total: unknown
//! hashes and garbage bytes are error outcomes, never panics.
//!
//! The streets document's identity: the streets document is the
//! host's, and its hash is the streets' — the module answers the
//! same hash for the import, for the document handed back (as JSON
//! or on bulk op 1), and for the same streets read from a differently
//! ordered file. A document that is not canonical is refused, never
//! registered under a second hash.

use cap_streets::{dispatch, dispatch_bytes, dispatch_json, Request, Response, OP_IMPORT, OP_LOAD_STREETS};
use serde_json::{json, Value};

/// The Elbow: one horizontal street, one vertical — routing A→B
/// must bend at the corner node, and the committed geometry starts
/// there (the trace follows streets, not chords).
const ELBOW: &str = r#"{"type":"FeatureCollection","features":[
    {"type":"Feature","geometry":{"type":"LineString",
        "coordinates":[[-73.5030,45.5040],[-73.5080,45.5040],[-73.5080,45.5090]]}}
]}"#;

fn load_elbow() -> (String, u32, u32) {
    match dispatch(Request::Import { bytes: ELBOW.as_bytes().to_vec() }).unwrap() {
        Response::Import { streets_hash, segments, nodes, .. } => (streets_hash, segments, nodes),
        _ => panic!("import"),
    }
}

#[test]
fn load_registers_and_reports_the_graph() {
    let (streets_hash, segments, nodes) = load_elbow();
    assert_eq!((segments, nodes), (2, 3), "two segments, three nodes (the corner welds)");
    // Idempotent: the same streets are the same graph, one registry entry.
    let again = match dispatch(Request::Import { bytes: ELBOW.as_bytes().to_vec() }).unwrap() {
        Response::Import { streets_hash: hash, segments, nodes, .. } => (hash, segments, nodes),
        _ => panic!("reimport"),
    };
    assert_eq!(again, (streets_hash.clone(), segments, nodes), "same streets, same hash");
    assert!(streets_hash.starts_with("sha256:"), "content-hash identity");
}

#[test]
fn snap_and_route_follow_street_geometry() {
    let (streets_hash, _, _) = load_elbow();
    let a = match dispatch(Request::Snap {
        streets_hash: streets_hash.clone(),
        lat: 45.5040,
        lon: -73.5030,
    })
    .unwrap()
    {
        Response::Snap { position: Some(p) } => p,
        _ => panic!("snap a"),
    };
    assert!(a.dist_m < 1e-6, "the click lands on the street");
    let b = match dispatch(Request::Snap {
        streets_hash: streets_hash.clone(),
        lat: 45.5080,
        lon: -73.5080,
    })
    .unwrap()
    {
        Response::Snap { position: Some(p) } => p,
        _ => panic!("snap b"),
    };
    match dispatch(Request::Route { streets_hash, from: a, to: b }).unwrap() {
        Response::Route { points, length_m } => {
            // Around the Elbow's corner: 0.005° of longitude (~390 m
            // at 45.5°N) plus 0.004° of latitude (~445 m) — about
            // 835 m along the streets, where the diagonal is ~592 m.
            let length = length_m.expect("a path has a length");
            assert!((800.0..870.0).contains(&length), "the street length, not the diagonal: {length} m");
            assert_eq!(points.len(), 2, "the corner, then the target");
            assert!((points[0].lat - 45.5040).abs() < 1e-9, "first bend is the corner");
            assert!((points[0].lon + 73.5080).abs() < 1e-9, "the corner, not the diagonal");
            assert!((points[1].lat - 45.5080).abs() < 1e-9, "ends exactly at the target");
        }
        _ => panic!("route"),
    }
}

#[test]
fn snap_far_from_the_network_answers_null() {
    let (streets_hash, _, _) = load_elbow();
    match dispatch(Request::Snap { streets_hash, lat: 44.0, lon: -70.0 }).unwrap() {
        Response::Snap { position: None } => {}
        _ => panic!("no street nearby answers null, not an error"),
    }
}

#[test]
fn route_between_disconnected_parts_answers_empty() {
    // Nowhere Rd: an island — the Elbow and it share no nodes.
    const ISLANDS: &str = r#"{"type":"FeatureCollection","features":[
        {"type":"Feature","geometry":{"type":"LineString",
            "coordinates":[[-73.5030,45.5040],[-73.5080,45.5040],[-73.5080,45.5090]]}},
        {"type":"Feature","geometry":{"type":"LineString",
            "coordinates":[[-73.6000,45.6000],[-73.6100,45.6100]]}}
    ]}"#;
    let streets_hash = match dispatch(Request::Import { bytes: ISLANDS.as_bytes().to_vec() })
        .unwrap()
    {
        Response::Import { streets_hash, .. } => streets_hash,
        _ => panic!("import"),
    };
    let a = match dispatch(Request::Snap { streets_hash: streets_hash.clone(), lat: 45.5040, lon: -73.5030 }).unwrap() {
        Response::Snap { position: Some(p) } => p,
        _ => panic!("snap a"),
    };
    let b = match dispatch(Request::Snap { streets_hash: streets_hash.clone(), lat: 45.6000, lon: -73.6000 }).unwrap() {
        Response::Snap { position: Some(p) } => p,
        _ => panic!("snap b"),
    };
    match dispatch(Request::Route { streets_hash, from: a, to: b }).unwrap() {
        Response::Route { points, length_m } => {
            assert!(points.is_empty(), "no path: empty, the caller falls back");
            assert_eq!(length_m, None, "no path, no length");
        }
        _ => panic!("route"),
    }
}

#[test]
fn the_json_seam_round_trips_and_errors_totally() {
    let (streets_hash, _, _) = load_elbow();
    // The typed request crosses as JSON and answers identically.
    let text = format!(
        r#"{{"op":"snap","streets_hash":"{streets_hash}","lat":45.5040,"lon":-73.5030}}"#
    );
    let outcome = dispatch_json(&text);
    assert!(outcome.contains(r#""ok":{"op":"snap""#), "{outcome}");
    assert!(outcome.contains(r#""dist_m":0.0"#), "the snapped position serializes: {outcome}");

    // Unknown hashes are error outcomes, never panics.
    let missing = dispatch_json(r#"{"op":"snap","streets_hash":"sha256:missing","lat":0.0,"lon":0.0}"#);
    assert!(missing.contains("unknown street network"), "{missing}");

    // Garbage bytes are an error outcome too (the bulk seam).
    let bad = dispatch_bytes(0, b"not geojson");
    assert!(bad.contains("street layer"), "{bad}");
    // Unknown bulk op codes error, never trap.
    let unknown = dispatch_bytes(9, ELBOW.as_bytes());
    assert!(unknown.contains("unknown bulk op"), "{unknown}");
    // Invalid JSON requests error through the standard outcome.
    let invalid = dispatch_json("{");
    assert!(invalid.contains("invalid request"), "{invalid}");
}

// ---------------------------------------------------------------------
// The document's identity
// ---------------------------------------------------------------------

const FIXTURE: &str = include_str!("../../../web/e2e/fixtures/streets.geojson");

fn ok(outcome: String) -> Value {
    let v: Value = serde_json::from_str(&outcome).unwrap();
    assert!(v.get("error").is_none(), "unexpected error: {v}");
    v["ok"].clone()
}

fn err(outcome: String) -> String {
    let v: Value = serde_json::from_str(&outcome).unwrap();
    v["error"].as_str().unwrap_or_else(|| panic!("expected an error, got {v}")).to_string()
}

#[test]
fn the_document_handed_back_is_the_same_hash() {
    let imported = ok(dispatch_bytes(OP_IMPORT, FIXTURE.as_bytes()));
    assert_eq!(imported["op"], "import");
    let hash = imported["streets_hash"].as_str().unwrap().to_string();
    assert!(hash.starts_with("sha256:"));
    assert_eq!(imported["ways"], imported["streets"]["ways"].as_array().unwrap().len());

    let streets = imported["streets"].clone();
    let by_json = ok(dispatch_json(&json!({ "op": "load_streets", "streets": streets }).to_string()));
    assert_eq!(by_json["streets_hash"], hash.as_str());
    let by_bytes = ok(dispatch_bytes(OP_LOAD_STREETS, streets.to_string().as_bytes()));
    assert_eq!(by_bytes["streets_hash"], hash.as_str());
    assert_eq!(by_bytes["segments"], imported["segments"]);
    assert_eq!(by_bytes["nodes"], imported["nodes"]);
}

#[test]
fn the_same_streets_from_another_file_are_the_same_hash() {
    let mut layer: Value = serde_json::from_str(FIXTURE).unwrap();
    let a = ok(dispatch_bytes(OP_IMPORT, layer.to_string().as_bytes()))["streets_hash"].clone();
    let features = layer["features"].as_array_mut().unwrap();
    features.reverse();
    for f in features.iter_mut() {
        f["properties"]["OBJECTID"] = json!(7);
    }
    let b = ok(dispatch_bytes(OP_IMPORT, layer.to_string().as_bytes()))["streets_hash"].clone();
    assert_eq!(a, b);
}

#[test]
fn queries_answer_over_a_loaded_document() {
    let imported = ok(dispatch_bytes(OP_IMPORT, FIXTURE.as_bytes()));
    let hash = imported["streets_hash"].as_str().unwrap();
    let a = ok(dispatch_json(&json!({ "op": "snap", "streets_hash": hash, "lat": 45.504, "lon": -73.503 }).to_string()));
    let b = ok(dispatch_json(&json!({ "op": "snap", "streets_hash": hash, "lat": 45.508, "lon": -73.508 }).to_string()));
    let routed = ok(dispatch_json(
        &json!({ "op": "route", "streets_hash": hash, "from": a["position"], "to": b["position"] }).to_string(),
    ));
    assert!(!routed["points"].as_array().unwrap().is_empty());
    assert!(routed["length_m"].as_f64().unwrap() > 0.0);
}

#[test]
fn what_is_not_canonical_or_not_streets_is_an_error() {
    let unsorted = json!({ "ways": [
        { "points": [[45601000, -122600000], [45601000, -122599000]], "class": "local" },
        { "points": [[45600000, -122600000], [45601000, -122600000]], "class": "local" },
    ]});
    assert!(err(dispatch_bytes(OP_LOAD_STREETS, unsorted.to_string().as_bytes())).contains("streets document"));
    assert!(err(dispatch_bytes(OP_LOAD_STREETS, b"\xff\xfe")).contains("streets document"));
    assert!(err(dispatch_bytes(OP_IMPORT, b"\xff\xfe")).contains("street layer"));
    let rails = json!({ "type": "FeatureCollection", "features": [
        { "type": "Feature", "properties": { "CFCC": "B10" },
          "geometry": { "type": "LineString", "coordinates": [[-122.6, 45.6], [-122.6, 45.601]] } },
    ]});
    assert!(err(dispatch_bytes(OP_IMPORT, rails.to_string().as_bytes())).contains("no streets"));
    assert!(err(dispatch_bytes(9, b"{}")).contains("unknown bulk op"));
    let unknown = dispatch_json(&json!({ "op": "snap", "streets_hash": "sha256:missing", "lat": 0, "lon": 0 }).to_string());
    assert!(err(unknown).contains("unknown street network"));
}

// ---------------------------------------------------------------------
// The walk table over a registered network
// ---------------------------------------------------------------------

const NETWORK_LOAD: &str = include_str!("../../../web/public/caps/sample-network-load.json");

fn load_network() -> String {
    let network: Value = serde_json::from_str::<Value>(NETWORK_LOAD).unwrap()["network"].clone();
    let by_bytes = ok(dispatch_bytes(cap_abi::OP_LOAD_NETWORK, network.to_string().as_bytes()));
    let by_json = ok(dispatch_json(&json!({ "op": "load_network", "network": network }).to_string()));
    assert_eq!(by_bytes["network_hash"], by_json["network_hash"], "either lane, one identity");
    by_bytes["network_hash"].as_str().unwrap().to_string()
}

#[test]
fn the_walk_table_names_what_it_walks_and_is_its_hash() {
    let network_hash = load_network();
    let streets_hash = ok(dispatch_bytes(OP_IMPORT, FIXTURE.as_bytes()))["streets_hash"].as_str().unwrap().to_string();
    let answer = ok(dispatch_json(
        &json!({ "op": "walks", "streets_hash": streets_hash, "network_hash": network_hash }).to_string(),
    ));
    let table = &answer["walks"];
    assert_eq!(table["streets_hash"], streets_hash.as_str());
    assert_eq!(table["network_hash"], network_hash.as_str());
    assert_eq!(table["speed_mps"], 1.3, "the default speed");
    assert_eq!(table["max_walk_secs"], 600, "the default limit: ten minutes");
    let typed: transit_streets::walking::WalkTable = serde_json::from_value(table.clone()).unwrap();
    assert_eq!(answer["walks_hash"], cap_abi::canonical_hash(&typed).unwrap().as_str());

    let slower = ok(dispatch_json(
        &json!({ "op": "walks", "streets_hash": streets_hash, "network_hash": network_hash,
                 "speed_mps": 0.9, "max_walk_secs": 900 }).to_string(),
    ));
    assert_eq!(slower["walks"]["speed_mps"], 0.9);
    assert_ne!(slower["walks_hash"], answer["walks_hash"], "the parameters are part of the document");

    let walks = |extra: Value| {
        let mut req = json!({ "op": "walks", "streets_hash": streets_hash, "network_hash": network_hash });
        req.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        err(dispatch_json(&req.to_string()))
    };
    assert!(walks(json!({ "speed_mps": 0.0 })).contains("walking speed"));
    assert!(walks(json!({ "max_walk_secs": 7200 })).contains("at most an hour"));
    assert!(walks(json!({ "network_hash": "sha256:missing" })).contains("load_network first"));
    assert!(walks(json!({ "streets_hash": "sha256:missing" })).contains("unknown street network"));
}
