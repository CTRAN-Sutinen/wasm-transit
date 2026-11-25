//! A layer's own schemes are read here and nowhere else: the graph and every
//! query read the document, never a layer's properties (D-30). TIGER's CFCC
//! is the scheme that the agency's Trapeze export uses.

use serde_json::Value;

use crate::model::{StreetClass, Streets, Way};
use crate::StreetError;

#[derive(Clone, Copy, PartialEq)]
enum Dir {
    Both,
    Forward,
    Backward,
}

fn prop<'a>(props: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| props.get(*k))
}

/// Unrecognized values are two-way, because schemas drift and a wrong
/// one-way is worse than none.
fn direction(props: &serde_json::Map<String, Value>) -> Dir {
    let raw = prop(props, &["OneWay", "oneway", "ONEWAY"]).and_then(Value::as_str);
    match raw.map(|r| r.trim().to_ascii_uppercase()).as_deref() {
        Some(">" | "F" | "FT" | "YES" | "TRUE" | "1") => Dir::Forward,
        Some("<" | "T" | "TF" | "REVERSE" | "-1") => Dir::Backward,
        _ => Dir::Both,
    }
}

fn class(props: &serde_json::Map<String, Value>) -> Option<StreetClass> {
    use StreetClass::*;
    if let Some(cfcc) = prop(props, &["CFCC", "cfcc"]) {
        let Some(code) = cfcc.as_str().map(|c| c.trim().to_ascii_uppercase()) else {
            return Some(Unknown);
        };
        return match code.get(..1) {
            // Not streets: railroads, landmarks, boundaries, water.
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

fn e6(x: f64, limit: f64) -> Option<i32> {
    let v = (x * 1e6).round();
    (v.is_finite() && v.abs() <= limit * 1e6).then_some(v as i32)
}

/// A malformed vertex breaks the line into two runs, so it leaves a gap and
/// not a segment.
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
