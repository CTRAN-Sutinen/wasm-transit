//! Native dispatch tests for the street-graph capability — the seam
//! without wasm. The Elbow route must follow street geometry (the
//! corner, not the diagonal), and the seam must be total: unknown
//! hashes and garbage bytes are error outcomes, never panics.

use cap_streets::{dispatch, dispatch_bytes, dispatch_json, Request, Response};

/// The Elbow: one horizontal street, one vertical — routing A→B
/// must bend at the corner node, and the committed geometry starts
/// there (the trace follows streets, not chords).
const ELBOW: &str = r#"{"type":"FeatureCollection","features":[
    {"type":"Feature","geometry":{"type":"LineString",
        "coordinates":[[-73.5030,45.5040],[-73.5080,45.5040],[-73.5080,45.5090]]}}
]}"#;

fn load_elbow() -> (String, u32, u32) {
    match dispatch(Request::LoadStreets { bytes: ELBOW.as_bytes().to_vec() }).unwrap() {
        Response::LoadStreets { streets_hash, segments, nodes } => (streets_hash, segments, nodes),
        _ => panic!("load_streets"),
    }
}

#[test]
fn load_registers_and_reports_the_graph() {
    let (streets_hash, segments, nodes) = load_elbow();
    assert_eq!((segments, nodes), (2, 3), "two segments, three nodes (the corner welds)");
    // Idempotent: the same bytes are the same graph, one registry entry.
    let again = match dispatch(Request::LoadStreets { bytes: ELBOW.as_bytes().to_vec() }).unwrap() {
        Response::LoadStreets { streets_hash: hash, segments, nodes } => (hash, segments, nodes),
        _ => panic!("reload"),
    };
    assert_eq!(again, (streets_hash.clone(), segments, nodes), "same bytes, same hash");
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
    let streets_hash = match dispatch(Request::LoadStreets { bytes: ISLANDS.as_bytes().to_vec() })
        .unwrap()
    {
        Response::LoadStreets { streets_hash, .. } => streets_hash,
        _ => panic!("load"),
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
    assert!(bad.contains("street network"), "{bad}");
    // Unknown bulk op codes error, never trap.
    let unknown = dispatch_bytes(9, ELBOW.as_bytes());
    assert!(unknown.contains("unknown bulk op"), "{unknown}");
    // Invalid JSON requests error through the standard outcome.
    let invalid = dispatch_json("{");
    assert!(invalid.contains("invalid request"), "{invalid}");
}
