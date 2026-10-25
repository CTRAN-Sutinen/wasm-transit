use transit_streets::{StreetNetwork, StreetPoint};

const ELBOW: &str = r#"{
  "type": "FeatureCollection",
  "features": [
    { "type": "Feature", "properties": { "name": "Elbow Ln" },
      "geometry": { "type": "LineString",
        "coordinates": [[-73.5030, 45.5040], [-73.5080, 45.5040], [-73.5080, 45.5090]] } }
  ]
}"#;

fn parse(geojson: &str) -> StreetNetwork {
    StreetNetwork::parse(geojson.as_bytes()).expect("parse")
}

#[test]
fn elbow_route_follows_the_street_not_the_diagonal() {
    let net = parse(ELBOW);
    // Start at Elbow's first vertex, end mid vertical leg: the
    // shortest street path turns at the corner (-73.5080, 45.5040);
    // a straight line would cut the diagonal.
    let a = net.snap(45.5040, -73.5030).expect("snap a");
    let b = net.snap(45.5080, -73.5080).expect("snap b");
    let route = net.route(a, b).expect("route");
    assert_eq!(
        route,
        vec![
            StreetPoint { lat: 45.5040, lon: -73.5080 }, // the corner
            StreetPoint { lat: 45.5080, lon: -73.5080 }, // the endpoint
        ]
    );
}

#[test]
fn snap_projects_and_measures_distance() {
    let net = parse(ELBOW);
    // A click ~55 m north of the horizontal leg snaps onto it, at
    // distance (snapping's neighborhood is a few ~110 m cells).
    let hit = net.snap(45.5045, -73.5050).expect("snap");
    assert!((hit.lat - 45.5040).abs() < 1e-9, "on the leg: {hit:?}");
    assert!(hit.dist_m > 50.0 && hit.dist_m < 60.0, "meters: {hit:?}");
    assert_eq!(hit.seg, 0);
    assert!((hit.t - 0.4).abs() < 1e-6, "mid-leg: {hit:?}");
}

#[test]
fn same_segment_route_is_the_endpoint() {
    let net = parse(ELBOW);
    let a = net.snap(45.5040, -73.5040).expect("a");
    let b = net.snap(45.5045, -73.5060).expect("b");
    assert_eq!(a.seg, b.seg, "both on the horizontal leg");
    assert_eq!(
        net.route(a, b),
        Some(vec![StreetPoint { lat: b.lat, lon: b.lon }]),
        "straight inside the segment"
    );
}

#[test]
fn disconnected_parts_do_not_route() {
    let net = parse(
        r#"{ "type": "FeatureCollection", "features": [
          { "type": "Feature", "geometry": { "type": "LineString",
            "coordinates": [[-73.50, 45.50], [-73.51, 45.50]] } },
          { "type": "Feature", "geometry": { "type": "LineString",
            "coordinates": [[-73.60, 45.60], [-73.61, 45.61]] } }
        ] }"#,
    );
    let a = net.snap(45.50, -73.50).expect("a");
    let b = net.snap(45.60, -73.60).expect("b");
    assert_eq!(net.route(a, b), None, "islands never connect");
}

#[test]
fn shared_endpoints_weld_across_features() {
    let net = parse(
        r#"{ "type": "FeatureCollection", "features": [
          { "type": "Feature", "geometry": { "type": "LineString",
            "coordinates": [[-73.50, 45.50], [-73.52, 45.50]] } },
          { "type": "Feature", "geometry": { "type": "LineString",
            "coordinates": [[-73.52, 45.50], [-73.52, 45.52]] } }
        ] }"#,
    );
    // 3 nodes, not 4: the shared endpoint welded into one.
    assert_eq!(net.node_count(), 3);
    let a = net.snap(45.50, -73.50).expect("a");
    let b = net.snap(45.52, -73.52).expect("b");
    let route = net.route(a, b).expect("routed across the weld");
    assert_eq!(route.len(), 2, "corner + endpoint: {route:?}");
    assert!((route[0].lat - 45.50).abs() < 1e-9 && (route[0].lon + 73.52).abs() < 1e-9);
}

#[test]
fn junk_rows_and_degenerate_segments_are_skipped() {
    // Points, polygons, a zero-length "segment", and broken rows
    // contribute nothing; the valid line still loads.
    let net = parse(
        r#"{ "type": "FeatureCollection", "features": [
          { "type": "Feature", "geometry": { "type": "Point", "coordinates": [-73.5, 45.5] } },
          { "type": "Feature", "geometry": { "type": "Polygon",
            "coordinates": [[[-73.5, 45.5], [-73.5, 45.6], [-73.6, 45.6], [-73.5, 45.5]]] } },
          { "type": "Feature", "geometry": { "type": "LineString",
            "coordinates": [[-73.50, 45.50], [-73.50, 45.50], [-73.51, 45.51]] } }
        ] }"#,
    );
    assert_eq!(net.segment_count(), 1, "only the real segment survives");
    assert_eq!(net.node_count(), 2);
    // Quantization (~0.1 m) is the weld tolerance: differences under
    // 5e-7 degrees round together, 9e-7 ones do not.
    let near = parse(
        r#"{ "type": "FeatureCollection", "features": [
          { "type": "Feature", "geometry": { "type": "LineString",
            "coordinates": [[-73.5000000, 45.5000000], [-73.51, 45.50]] } },
          { "type": "Feature", "geometry": { "type": "LineString",
            "coordinates": [[-73.5000004, 45.5000000], [-73.50, 45.52]] } }
        ] }"#,
    );
    assert_eq!(near.node_count(), 3, "quantized endpoints weld");
    let apart = parse(
        r#"{ "type": "FeatureCollection", "features": [
          { "type": "Feature", "geometry": { "type": "LineString",
            "coordinates": [[-73.5000000, 45.5000000], [-73.51, 45.50]] } },
          { "type": "Feature", "geometry": { "type": "LineString",
            "coordinates": [[-73.5000009, 45.5000000], [-73.50, 45.52]] } }
        ] }"#,
    );
    assert_eq!(apart.node_count(), 4, "beyond the tolerance they do not");
}

#[test]
fn no_lines_is_an_error() {
    assert!(StreetNetwork::parse(br#"{"type":"FeatureCollection","features":[]}"#).is_err());
    assert!(StreetNetwork::parse(b"not json").is_err());
}

#[test]
fn multilinestring_features_contribute_all_their_lines() {
    let net = parse(
        r#"{ "type": "FeatureCollection", "features": [
          { "type": "Feature", "geometry": { "type": "MultiLineString",
            "coordinates": [
              [[-73.50, 45.50], [-73.52, 45.50]],
              [[-73.52, 45.50], [-73.52, 45.52]]
            ] } }
        ] }"#,
    );
    assert_eq!(net.segment_count(), 2);
    assert_eq!(net.node_count(), 3);
    let a = net.snap(45.50, -73.50).expect("a");
    let b = net.snap(45.52, -73.52).expect("b");
    assert!(net.route(a, b).is_some(), "lines of one feature connect");
}

/// A one-way couplet: Front St runs west-to-east only (TIGER `>`);
/// Back St is the two-way return. Legal travel east is direct; travel
/// west must loop around via Back St.
const ONEWAY: &str = r#"{
  "type": "FeatureCollection",
  "features": [
    { "type": "Feature", "properties": { "OneWay": ">" },
      "geometry": { "type": "LineString",
        "coordinates": [[-73.5100, 45.5040], [-73.5060, 45.5040]] } },
    { "type": "Feature", "properties": { "name": "Back St" },
      "geometry": { "type": "LineString",
        "coordinates": [[-73.5060, 45.5045], [-73.5100, 45.5045]] } },
    { "type": "Feature", "properties": { "name": "West connector" },
      "geometry": { "type": "LineString",
        "coordinates": [[-73.5100, 45.5040], [-73.5100, 45.5045]] } },
    { "type": "Feature", "properties": { "name": "East connector" },
      "geometry": { "type": "LineString",
        "coordinates": [[-73.5060, 45.5040], [-73.5060, 45.5045]] } }
  ] }"#;

#[test]
fn one_way_routes_with_the_digitized_direction() {
    let net = parse(ONEWAY);
    // West end to east end of Front St: one-way, direct.
    let a = net.snap(45.5040, -73.5100).expect("a");
    let b = net.snap(45.5040, -73.5060).expect("b");
    assert_eq!(a.seg, b.seg, "both on Front St");
    let route = net.route(a, b).expect("eastbound one-way is legal");
    assert_eq!(
        route,
        vec![StreetPoint { lat: 45.5040, lon: -73.5060 }],
        "same segment, target ahead: the endpoint only"
    );
}

#[test]
fn one_way_against_the_direction_takes_the_long_way() {
    let net = parse(ONEWAY);
    // East end back to west end: Front St is closed westbound, so
    // the route must leave Front St and return via Back St.
    let a = net.snap(45.5040, -73.5060).expect("a");
    let b = net.snap(45.5040, -73.5100).expect("b");
    let route = net.route(a, b).expect("a legal loop exists");
    let lons: Vec<f64> = route.iter().map(|p| p.lon).collect();
    assert!(
        lons.iter().any(|&lon| (lon - -73.5060).abs() < 1e-9),
        "leaves through the east connector: {route:?}"
    );
    assert!(
        route.iter().any(|p| (p.lat - 45.5045).abs() < 1e-9),
        "returns along Back St (lat 45.5045): {route:?}"
    );
    // And it never cuts straight across Front St westbound.
    assert!(
        !route.iter().any(|p| (p.lat - 45.5040).abs() < 1e-9
            && p.lon > -73.5100 + 1e-9 && p.lon < -73.5060 - 1e-9),
        "no interior point of Front St: {route:?}"
    );
}

#[test]
fn one_way_target_behind_is_not_a_shortcut() {
    // A lone one-way segment: a target behind the position can be
    // reached neither within the segment (walking backward) nor
    // around it (nothing else exists).
    let net = parse(
        r#"{
          "type": "FeatureCollection",
          "features": [
            { "type": "Feature", "properties": { "OneWay": ">" },
              "geometry": { "type": "LineString",
                "coordinates": [[-73.5100, 45.5040], [-73.5060, 45.5040]] } }
          ] }"#,
    );
    let a = net.snap(45.5040, -73.5070).expect("a");
    let b = net.snap(45.5040, -73.5090).expect("b");
    assert_eq!(a.seg, b.seg);
    assert!(b.t < a.t, "target sits behind the position (smaller t)");
    assert!(
        net.route(a, b).is_none(),
        "a one-way segment cannot be walked backward"
    );
    // Ahead of the position: the endpoint, direct.
    let c = net.snap(45.5040, -73.5065).expect("c");
    assert!(c.t > a.t, "target ahead");
    assert_eq!(
        net.route(a, c),
        Some(vec![StreetPoint { lat: c.lat, lon: c.lon }])
    );
}

#[test]
fn oneway_attribute_conventions_parse() {
    // OSM-style `oneway` (reverse = against the digitized direction)
    // and TIGER `<` must both close the segment against its grain.
    let net = parse(
        r#"{
          "type": "FeatureCollection",
          "features": [
            { "type": "Feature", "properties": { "oneway": "-1" },
              "geometry": { "type": "LineString",
                "coordinates": [[-73.5100, 45.5040], [-73.5060, 45.5040]] } },
            { "type": "Feature", "properties": { "OneWay": "<" },
              "geometry": { "type": "LineString",
                "coordinates": [[-73.5100, 45.5045], [-73.5060, 45.5045]] } }
          ] }"#,
    );
    // With the digitized direction: both closed.
    let a = net.snap(45.5040, -73.5100).expect("a");
    let b = net.snap(45.5040, -73.5060).expect("b");
    assert!(net.route(a, b).is_none(), "oneway -1 forbids west-to-east");
    // Against it: both open (same segment, target ahead).
    let r = net.route(b, a).expect("against the grain is the legal way");
    assert_eq!(
        r,
        vec![StreetPoint { lat: 45.5040, lon: -73.5100 }]
    );
    // The `<` line, digitized west-to-east, is likewise open only
    // against its grain: east-to-west.
    let c = net.snap(45.5045, -73.5060).expect("c");
    let d = net.snap(45.5045, -73.5100).expect("d");
    assert!(net.route(c, d).is_some(), "< forbids with the grain");
    assert!(net.route(d, c).is_none(), "< allows only against the grain");
}

#[test]
fn unrecognized_oneway_values_stay_two_way() {
    let net = parse(
        r#"{
          "type": "FeatureCollection",
          "features": [
            { "type": "Feature", "properties": { "OneWay": "left turns from the bakery" },
              "geometry": { "type": "LineString",
                "coordinates": [[-73.5100, 45.5040], [-73.5060, 45.5040]] } }
          ] }"#,
    );
    let a = net.snap(45.5040, -73.5100).expect("a");
    let b = net.snap(45.5040, -73.5060).expect("b");
    assert!(net.route(a, b).is_some());
    assert!(net.route(b, a).is_some());
}
