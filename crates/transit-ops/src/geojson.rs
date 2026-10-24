//! Typed GeoJSON responses for map rendering.
//!
//! Concrete structs, not `serde_json::Value`: hosts that serialize
//! through serde-wasm-bindgen turn generic maps into JS `Map` objects
//! (which MapLibre and JSON.stringify don't understand), while
//! structs become plain objects. GeoJSON coordinate order is
//! [lon, lat].

use ts_rs::TS;
use serde::Serialize;

use transit_core::{Route, Stop, StopSummary};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FeatureCollection {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub features: Vec<StopFeature>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopFeature {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub geometry: PointGeometry,
    pub properties: StopProps,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PointGeometry {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub coordinates: [f64; 2],
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopProps {
    pub stop_id: String,
    pub name: String,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TripLineFeature {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub geometry: LineGeometry,
    pub properties: TripProps,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LineGeometry {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub coordinates: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TripProps {
    pub trip_id: String,
    pub route_id: Option<String>,
    pub route_name: Option<String>,
}

impl StopProps {
    pub fn of(stop: &Stop) -> StopProps {
        StopProps {
            stop_id: stop.stop_id.clone(),
            name: stop.stop_name.clone(),
            code: stop.stop_code.clone(),
        }
    }
}

impl TripProps {
    pub fn of(trip_id: &str, route: Option<&Route>) -> TripProps {
        TripProps {
            trip_id: trip_id.to_string(),
            route_id: route.map(|r| r.route_id.clone()),
            route_name: route.map(|r| r.display_name()),
        }
    }
}

/// All stops as a FeatureCollection, for map rendering.
pub fn stops_geojson(gtfs: &transit_core::Gtfs) -> FeatureCollection {
    let mut features = Vec::new();
    // Sorted by stop_id for deterministic output.
    let mut stops: Vec<&Stop> = gtfs.stops.values().collect();
    stops.sort_by(|a, b| a.stop_id.cmp(&b.stop_id));
    for stop in stops {
        let (Some(lat), Some(lon)) = (stop.stop_lat, stop.stop_lon) else { continue };
        features.push(StopFeature {
            kind: "Feature",
            geometry: PointGeometry { kind: "Point", coordinates: [lon, lat] },
            properties: StopProps::of(stop),
        });
    }
    FeatureCollection { kind: "FeatureCollection", features }
}

/// One stop as a point Feature (or None when it has no geometry or is
/// unknown), for drawing straight walking lines in trip plans.
pub fn stop_point(gtfs: &transit_core::Gtfs, stop_id: &str) -> Option<StopFeature> {
    let stop = gtfs.stops.get(stop_id)?;
    let (lat, lon) = (stop.stop_lat?, stop.stop_lon?);
    Some(StopFeature {
        kind: "Feature",
        geometry: PointGeometry { kind: "Point", coordinates: [lon, lat] },
        properties: StopProps::of(stop),
    })
}

/// A (lat, lon) polyline as a trip-line Feature; None when it has
/// fewer than two points (MapLibre can't draw that).
pub fn trip_line_feature(
    gtfs: &transit_core::Gtfs,
    trip_id: &str,
    path: &[(f64, f64)],
) -> Option<TripLineFeature> {
    if path.len() < 2 {
        return None;
    }
    Some(TripLineFeature {
        kind: "Feature",
        geometry: LineGeometry {
            kind: "LineString",
            coordinates: path.iter().map(|(lat, lon)| [*lon, *lat]).collect(),
        },
        properties: TripProps::of(trip_id, gtfs.trips.get(trip_id).map(|t| &gtfs.routes[&t.route_id])),
    })
}

/// `[{ stop_id, name }]` for UI pickers — the plain summary form the
/// pickers and the route-pattern views share.
pub fn stop_summaries(gtfs: &transit_core::Gtfs) -> Vec<StopSummary> {
    gtfs.stop_summaries()
}
