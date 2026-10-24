//! transit-wasm — the JavaScript boundary.
//!
//! Contract with the JS side:
//!
//! * JS owns all networking and file access. It fetches/drops the GTFS
//!   zip and the GTFS-rt protobuf, and hands us raw bytes.
//! * We parse those bytes in Rust and hand back plain data (counts,
//!   departure boards, trip updates) via `serde-wasm-bindgen`, so JS
//!   never sees anything but JSON-shaped values.
//! * JS also owns the clock: wasm has no wall time, so the caller passes
//!   the service date ("YYYY-MM-DD") and seconds-since-midnight it wants
//!   to look from.

use std::cell::RefCell;

use chrono::NaiveDate;
use serde::Serialize;
use wasm_bindgen::prelude::*;
/// A parsed static feed, held by JavaScript for the lifetime of the page.
///
/// The inner `Gtfs` is *not* copied across the boundary per call — JS
/// keeps one handle and calls methods on it. This is the local-first
/// trick: the whole timetable lives in wasm memory, offline.
#[wasm_bindgen]
pub struct GtfsHandle {
    gtfs: transit_core::Gtfs,
    /// Routing network, built lazily for the first queried service
    /// date and rebuilt if the date changes (the network is
    /// date-specific: only trips running that day are in it).
    router: RefCell<Option<(String, transit_router::RouterNetwork)>>,
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(value).map_err(|e| JsError::new(&e.to_string()))
}

fn parse_date(s: &str) -> Result<NaiveDate, JsError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| JsError::new(&format!("invalid date {s:?}, expected YYYY-MM-DD: {e}")))
}

/// GeoJSON wrappers for map rendering.
///
/// Typed structs, not `serde_json::Value`: serde-wasm-bindgen turns
/// generic maps into JS `Map` objects (which MapLibre and JSON.stringify
/// don't understand), while structs become plain objects.
#[derive(Serialize)]
struct GeoJsonFeatureCollection<T> {
    #[serde(rename = "type")]
    kind: &'static str,
    features: Vec<T>,
}

#[derive(Serialize)]
struct GeoJsonFeature<G, P> {
    #[serde(rename = "type")]
    kind: &'static str,
    geometry: G,
    properties: P,
}

#[derive(Serialize)]
struct GeoJsonPoint {
    #[serde(rename = "type")]
    kind: &'static str,
    coordinates: [f64; 2],
}

#[derive(Serialize)]
struct GeoJsonLineString {
    #[serde(rename = "type")]
    kind: &'static str,
    coordinates: Vec<[f64; 2]>,
}

#[derive(Serialize)]
struct StopProps {
    stop_id: String,
    name: String,
    code: Option<String>,
}

#[derive(Serialize)]
struct TripProps {
    trip_id: String,
    route_id: Option<String>,
    route_name: Option<String>,
}

#[wasm_bindgen]
impl GtfsHandle {
    /// Parse a GTFS static zip from raw bytes.
    pub fn load(bytes: &[u8]) -> Result<GtfsHandle, JsError> {
        transit_core::Gtfs::from_zip_bytes(bytes)
            .map(|gtfs| GtfsHandle { gtfs, router: RefCell::new(None) })
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Entity counts: { agencies, routes, stops, trips, stop_times }
    pub fn summary(&self) -> Result<JsValue, JsError> {
        to_js(&self.gtfs.summary())
    }

    /// Name-sorted list of [{ stop_id, name }] for UI pickers.
    pub fn stops(&self) -> Result<JsValue, JsError> {
        to_js(&self.gtfs.stop_summaries())
    }

    /// Upcoming departures at a stop.
    ///
    /// `date` is the service date ("YYYY-MM-DD"), `from_secs` is seconds
    /// since midnight of that service day (allowing values past 86400 for
    /// "night of" queries).
    pub fn departures(
        &self,
        stop_id: String,
        date: String,
        from_secs: u32,
        limit: u32,
    ) -> Result<JsValue, JsError> {
        let date = parse_date(&date)?;
        Ok(to_js(
            &self.gtfs.next_departures(&stop_id, date, from_secs, limit as usize),
        )?)
    }

    /// All blocks in the feed: [{ block_id, trip_count }], sorted by id.
    pub fn blocks(&self) -> Result<JsValue, JsError> {
        to_js(&self.gtfs.blocks())
    }

    /// Service keys running on a date (often several): ["WK", "HOL"].
    pub fn active_services(&self, date: String) -> Result<JsValue, JsError> {
        let date = parse_date(&date)?;
        to_js(&self.gtfs.active_services(date))
    }

    /// All stops as a GeoJSON FeatureCollection (points carry stop_id,
    /// name and code as properties) for map rendering. GeoJSON
    /// coordinate order is [lon, lat].
    pub fn stops_geojson(&self) -> Result<JsValue, JsError> {
        let features: Vec<GeoJsonFeature<GeoJsonPoint, StopProps>> = self
            .gtfs
            .stops
            .values()
            .filter_map(|stop| {
                let (lat, lon) = (stop.stop_lat?, stop.stop_lon?);
                Some(GeoJsonFeature {
                    kind: "Feature",
                    geometry: GeoJsonPoint { kind: "Point", coordinates: [lon, lat] },
                    properties: StopProps {
                        stop_id: stop.stop_id.clone(),
                        name: stop.stop_name.clone(),
                        code: stop.stop_code.clone(),
                    },
                })
            })
            .collect();
        to_js(&GeoJsonFeatureCollection { kind: "FeatureCollection", features })
    }

    /// One trip's path as a GeoJSON Feature with a LineString geometry —
    /// shapes.txt polyline if available, else stop-to-stop segments.
    /// Null when the trip has no usable geometry.
    pub fn trip_path_geojson(&self, trip_id: String) -> Result<JsValue, JsError> {
        let path = self.gtfs.trip_path(&trip_id);
        if path.len() < 2 {
            return to_js(&Option::<GeoJsonFeature<GeoJsonLineString, TripProps>>::None);
        }
        let coordinates: Vec<[f64; 2]> =
            path.iter().map(|(lat, lon)| [*lon, *lat]).collect();
        let trip = self.gtfs.trips.get(&trip_id);
        to_js(&Some(GeoJsonFeature {
            kind: "Feature",
            geometry: GeoJsonLineString { kind: "LineString", coordinates },
            properties: TripProps {
                trip_id,
                route_id: trip.map(|t| t.route_id.clone()),
                route_name: trip
                    .and_then(|t| self.gtfs.routes.get(&t.route_id))
                    .map(|r| r.display_name()),
            },
        }))
    }

    /// One stop as a GeoJSON point Feature (or null), for drawing
    /// straight walking lines in trip plans.
    pub fn stop_point(&self, stop_id: String) -> Result<JsValue, JsError> {
        let feature = self.gtfs.stops.get(&stop_id).and_then(|stop| {
            let (lat, lon) = (stop.stop_lat?, stop.stop_lon?);
            Some(GeoJsonFeature {
                kind: "Feature",
                geometry: GeoJsonPoint { kind: "Point", coordinates: [lon, lat] },
                properties: StopProps {
                    stop_id: stop.stop_id.clone(),
                    name: stop.stop_name.clone(),
                    code: stop.stop_code.clone(),
                },
            })
        });
        to_js(&feature)
    }

    /// One leg of a trip (board stop → alight stop) as a GeoJSON
    /// LineString Feature — the ridden segment only, not the whole
    /// route. Null when there is no usable geometry.
    pub fn trip_segment_geojson(
        &self,
        trip_id: String,
        from_stop_id: String,
        to_stop_id: String,
    ) -> Result<JsValue, JsError> {
        let path = self.gtfs.trip_path_segment(&trip_id, &from_stop_id, &to_stop_id);
        if path.len() < 2 {
            return to_js(&Option::<GeoJsonFeature<GeoJsonLineString, TripProps>>::None);
        }
        let coordinates: Vec<[f64; 2]> =
            path.iter().map(|(lat, lon)| [*lon, *lat]).collect();
        let trip = self.gtfs.trips.get(&trip_id);
        to_js(&Some(GeoJsonFeature {
            kind: "Feature",
            geometry: GeoJsonLineString { kind: "LineString", coordinates },
            properties: TripProps {
                trip_id,
                route_id: trip.map(|t| t.route_id.clone()),
                route_name: trip
                    .and_then(|t| self.gtfs.routes.get(&t.route_id))
                    .map(|r| r.display_name()),
            },
        }))
    }

    /// Earliest-arrival trip plan (RAPTOR): from/to stop ids, service
    /// date, departure time as seconds since that date's midnight, and a
    /// maximum number of transit legs. Returns the itinerary (legs with
    /// board/alight times and stops) or null if unreachable within the
    /// leg budget. The routing network is built once per date and
    /// cached.
    pub fn plan_trip(
        &self,
        from: String,
        to: String,
        date: String,
        depart_secs: u32,
        max_legs: u32,
    ) -> Result<JsValue, JsError> {
        let parsed = parse_date(&date)?;
        let mut cache = self.router.borrow_mut();
        let stale = cache.as_ref().map(|(key, _)| key != &date).unwrap_or(true);
        if stale {
            *cache = Some((date, transit_router::RouterNetwork::build(&self.gtfs, parsed)));
        }
        let (_, net) = cache.as_ref().expect("router built");
        let itinerary = transit_router::earliest_arrival(
            net,
            &from,
            &to,
            depart_secs,
            max_legs.max(1) as usize,
        )
        .map_err(|e| JsError::new(&e.to_string()))?;
        to_js(&itinerary)
    }

    /// The trip chain of one block on a service date: the vehicle's
    /// day as [{ trip_id, route_name, start, end, layover_secs, ... }].
    pub fn block_trips(&self, block_id: String, date: String) -> Result<JsValue, JsError> {
        let date = parse_date(&date)?;
        to_js(&self.gtfs.block_trips(&block_id, date))
    }
}

// ---------------------------------------------------------------
// GTFS-realtime
// ---------------------------------------------------------------

/// Flat view of one trip update in a feed.
#[derive(Serialize)]
struct TripUpdateView {
    trip_id: Option<String>,
    route_id: Option<String>,
    stop_updates: Vec<StopUpdateView>,
}

#[derive(Serialize)]
struct StopUpdateView {
    stop_id: Option<String>,
    stop_sequence: Option<u32>,
    /// Seconds of delay; positive = late.
    arrival_delay: Option<i32>,
    departure_delay: Option<i32>,
}

/// One live vehicle from a VehiclePositions feed.
#[derive(Serialize)]
struct VehicleView {
    entity_id: String,
    trip_id: Option<String>,
    route_id: Option<String>,
    lat: f64,
    lon: f64,
    bearing: Option<f32>,
    stop_id: Option<String>,
}

/// Browser-friendly overview of a decoded GTFS-rt feed: header, entity
/// counts by type, and the trip updates (delays) for display.
#[derive(Serialize)]
struct RtOverview {
    gtfs_realtime_version: Option<String>,
    /// Unix seconds, as served in the feed header.
    timestamp: Option<u64>,
    entities: usize,
    trip_updates: usize,
    vehicles: usize,
    alerts: usize,
    /// Individual trip updates, capped for UI sanity.
    trips: Vec<TripUpdateView>,
    /// Live vehicles with positions, for the map.
    vehicle_positions: Vec<VehicleView>,
}

/// Rendered trip updates, capped for UI sanity. Kept generous: the UI
/// joins these onto departure boards and vehicle dots by trip_id, so a
/// low cap silently drops delay information.
const MAX_TRIPS_SHOWN: usize = 1000;

/// Decode a GTFS-rt protobuf payload into an overview object.
///
/// `gtfs_rt` types are also available directly for anything richer than
/// this view; keeping the JS surface small is a deliberate v1 choice.
#[wasm_bindgen]
pub fn decode_rt(bytes: &[u8]) -> Result<JsValue, JsError> {
    let msg = transit_rt::decode(bytes).map_err(|e| JsError::new(&e.to_string()))?;

    // gtfs-realtime.proto is proto2: `required` fields (header, and the
    // TripDescriptor inside a TripUpdate) are plain values in prost.
    let mut overview = RtOverview {
        gtfs_realtime_version: Some(msg.header.gtfs_realtime_version.clone()),
        timestamp: msg.header.timestamp,
        entities: msg.entity.len(),
        trip_updates: 0,
        vehicles: 0,
        alerts: 0,
        trips: Vec::new(),
        vehicle_positions: Vec::new(),
    };

    for entity in &msg.entity {
        if let Some(tu) = entity.trip_update.as_ref() {
            overview.trip_updates += 1;
            if overview.trips.len() < MAX_TRIPS_SHOWN {
                overview.trips.push(TripUpdateView {
                    trip_id: tu.trip.trip_id.clone(),
                    route_id: tu.trip.route_id.clone(),
                    stop_updates: tu
                        .stop_time_update
                        .iter()
                        .map(|s| StopUpdateView {
                            stop_id: s.stop_id.clone(),
                            stop_sequence: s.stop_sequence,
                            arrival_delay: s.arrival.as_ref().and_then(|e| e.delay),
                            departure_delay: s.departure.as_ref().and_then(|e| e.delay),
                        })
                        .collect(),
                });
            }
        }
        if let Some(vehicle) = entity.vehicle.as_ref() {
            overview.vehicles += 1;
            if let Some(position) = vehicle.position.as_ref() {
                overview.vehicle_positions.push(VehicleView {
                    entity_id: entity.id.clone(),
                    trip_id: vehicle.trip.as_ref().and_then(|t| t.trip_id.clone()),
                    route_id: vehicle.trip.as_ref().and_then(|t| t.route_id.clone()),
                    lat: position.latitude as f64,
                    lon: position.longitude as f64,
                    bearing: position.bearing,
                    stop_id: vehicle.stop_id.clone(),
                });
            }
        }
        if entity.alert.is_some() {
            overview.alerts += 1;
        }
    }

    to_js(&overview)
}
