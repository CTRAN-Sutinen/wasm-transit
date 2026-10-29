//! The street-layer import: GeoJSON lines in, the streets document
//! out. The boundary half of the crate — the graph and every query
//! read the document, never a layer's properties.
//!
//! A layer's own schemes are read here and nowhere else: the class
//! (TIGER's CFCC — the agency's Trapeze export spells it so — TIGER's
//! MTFCC, OSM's `highway`), the name (`FULLNAME`, `name`, `NAME`), and
//! the one-way conventions (TIGER `OneWay`, OSM `oneway`). Which lines
//! are streets and what class each is: D-30 (docs/DECISIONS.md).

use serde_json::Value;

use crate::model::{StreetClass, Streets, Way};
use crate::StreetError;

/// A layer's travel direction relative to its digitized order.
#[derive(Clone, Copy, PartialEq)]
enum Dir {
    Both,
    Forward,
    Backward,
}

/// Read a property case-insensitively over the spellings layers use.
fn prop<'a>(props: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| props.get(*k))
}

/// One-way convention, read case-insensitively: TIGER-style `OneWay`
/// (`>` with the digitized direction, `<` against; some derivatives
/// use F/T, FT/TF) and OSM-style `oneway` (`yes`/`true`/`1` with,
/// `-1`/`reverse` against, `no`/`false`/`0` two-way). Unrecognized
/// values are two-way: schemas drift, and a wrong one-way is worse
/// than none.
fn direction(props: &serde_json::Map<String, Value>) -> Dir {
    let raw = prop(props, &["OneWay", "oneway", "ONEWAY"]).and_then(Value::as_str);
    match raw.map(|r| r.trim().to_ascii_uppercase()).as_deref() {
        Some(">" | "F" | "FT" | "YES" | "TRUE" | "1") => Dir::Forward,
        Some("<" | "T" | "TF" | "REVERSE" | "-1") => Dir::Backward,
        _ => Dir::Both,
    }
}

/// D-30: the street's class, or None when the line is not a street.
fn class(props: &serde_json::Map<String, Value>) -> Option<StreetClass> {
    use StreetClass::*;
    if let Some(cfcc) = prop(props, &["CFCC", "cfcc"]) {
        let Some(code) = cfcc.as_str().map(|c| c.trim().to_ascii_uppercase()) else {
            return Some(Unknown);
        };
        return match code.get(..1) {
            // Railroads, other transport, landmarks, physical
            // features, boundaries, water.
            Some("B" | "C" | "D" | "E" | "F" | "H") => None,
            Some("A") => Some(match code.get(..2) {
                Some("A1") => Motorway,
                Some("A2") => Primary,
                Some("A3") => Secondary,
                Some("A4") => Local,
                Some("A5") => Track,
                _ => match code.as_str() {
                    "A60" | "A61" | "A62" => Local,
                    "A63" => Ramp,
                    "A64" | "A73" | "A74" => Service,
                    "A70" | "A71" => Path,
                    "A72" => Steps,
                    _ => Unknown,
                },
            }),
            _ => Some(Unknown),
        };
    }
    if let Some(mtfcc) = prop(props, &["MTFCC", "mtfcc"]) {
        let Some(code) = mtfcc.as_str().map(|c| c.trim().to_ascii_uppercase()) else {
            return Some(Unknown);
        };
        if !code.starts_with("S1") {
            return None;
        }
        return Some(match code.as_str() {
            "S1100" => Motorway,
            "S1200" => Primary,
            "S1400" => Local,
            "S1500" => Track,
            "S1630" => Ramp,
            "S1640" | "S1730" | "S1740" | "S1780" => Service,
            "S1710" | "S1820" | "S1830" => Path,
            "S1720" => Steps,
            _ => Unknown,
        });
    }
    if let Some(highway) = props.get("highway") {
        let value = highway.as_str().unwrap_or("");
        return Some(match value {
            "motorway" => Motorway,
            "trunk" | "primary" => Primary,
            "secondary" | "tertiary" => Secondary,
            "residential" | "unclassified" | "living_street" | "road" => Local,
            "service" => Service,
            "track" => Track,
            "footway" | "path" | "pedestrian" | "cycleway" | "bridleway" => Path,
            "steps" => Steps,
            v if v.ends_with("_link") => Ramp,
            _ => Unknown,
        });
    }
    if ["railway", "waterway", "natural", "power", "boundary"].iter().any(|k| props.contains_key(*k)) {
        return None;
    }
    Some(Unknown)
}

fn name(props: &serde_json::Map<String, Value>) -> Option<String> {
    ["FULLNAME", "name", "NAME"]
        .iter()
        .filter_map(|k| props.get(*k).and_then(Value::as_str))
        .find(|n| !n.is_empty())
        .map(str::to_string)
}

/// Degrees to microdegrees, when within `limit` degrees of zero.
fn e6(x: f64, limit: f64) -> Option<i32> {
    let v = (x * 1e6).round();
    (v.is_finite() && v.abs() <= limit * 1e6).then_some(v as i32)
}

/// One line's runs of well-formed vertices: a malformed vertex (not a
/// number pair, or off the globe) breaks
/// the line (a gap, not a segment), repeated vertices collapse, and a
/// run of fewer than two points is no street.
fn runs(line: &Value) -> Vec<Vec<[i32; 2]>> {
    let mut runs = vec![Vec::new()];
    for vertex in line.as_array().into_iter().flatten() {
        let row = vertex.as_array();
        let at = |i: usize| row.and_then(|r| r.get(i)).and_then(Value::as_f64);
        // GeoJSON order: [lon, lat].
        let point = at(1)
            .and_then(|lat| e6(lat, 90.0))
            .zip(at(0).and_then(|lon| e6(lon, 180.0)))
            .map(|(lat, lon)| [lat, lon]);
        match point {
            Some(p) => {
                let run: &mut Vec<[i32; 2]> = runs.last_mut().expect("runs is never empty");
                if run.last() != Some(&p) {
                    run.push(p);
                }
            }
            None => runs.push(Vec::new()),
        }
    }
    runs.retain(|r| r.len() >= 2);
    runs
}

/// Read a GeoJSON FeatureCollection (or bare Feature) of LineString /
/// MultiLineString streets into the streets document. Points,
/// polygons, malformed rows, and lines that are not streets (D-30) are
/// skipped. Errors when the input is not GeoJSON or holds no street.
pub fn import(geojson: &[u8]) -> Result<Streets, StreetError> {
    let value: Value =
        serde_json::from_slice(geojson).map_err(|e| StreetError(format!("invalid JSON: {e}")))?;
    let features = match value.get("type").and_then(Value::as_str) {
        Some("FeatureCollection") => value.get("features").and_then(Value::as_array).cloned().unwrap_or_default(),
        Some("Feature") => vec![value],
        _ => return Err(StreetError("expected a GeoJSON FeatureCollection or Feature".into())),
    };

    let empty = serde_json::Map::new();
    let mut ways = Vec::new();
    for feature in &features {
        let props = feature.get("properties").and_then(Value::as_object).unwrap_or(&empty);
        let Some(class) = class(props) else { continue };
        let Some(geometry) = feature.get("geometry") else { continue };
        let lines: Vec<&Value> = match geometry.get("type").and_then(Value::as_str) {
            Some("LineString") => geometry.get("coordinates").into_iter().collect(),
            Some("MultiLineString") => geometry
                .get("coordinates")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .collect(),
            _ => continue,
        };
        let dir = direction(props);
        let name = name(props);
        for mut points in lines.into_iter().flat_map(runs) {
            if dir == Dir::Backward {
                points.reverse();
            }
            ways.push(Way { points, oneway: dir != Dir::Both, class, name: name.clone() });
        }
    }
    if ways.is_empty() {
        return Err(StreetError(
            "no streets — expected LineString/MultiLineString street geometries".into(),
        ));
    }
    Ok(Streets::canonical(ways))
}
