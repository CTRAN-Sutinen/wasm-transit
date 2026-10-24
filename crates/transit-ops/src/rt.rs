//! Browser/server-friendly view of a decoded GTFS-rt feed: header,
//! entity counts by type, trip updates (delays), and live vehicles.
//! Serialization target of [`crate::Request::DecodeRt`].

use serde::Serialize;

/// Flat view of one trip update in a feed.
#[derive(Debug, Clone, Serialize)]
pub struct TripUpdateView {
    pub trip_id: Option<String>,
    pub route_id: Option<String>,
    pub stop_updates: Vec<StopUpdateView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StopUpdateView {
    pub stop_id: Option<String>,
    pub stop_sequence: Option<u32>,
    /// Seconds of delay; positive = late.
    pub arrival_delay: Option<i32>,
    pub departure_delay: Option<i32>,
}

/// One live vehicle from a VehiclePositions feed.
#[derive(Debug, Clone, Serialize)]
pub struct VehicleView {
    pub entity_id: String,
    pub trip_id: Option<String>,
    pub route_id: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub bearing: Option<f32>,
    pub stop_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RtOverview {
    pub gtfs_realtime_version: Option<String>,
    /// Unix seconds, as served in the feed header.
    pub timestamp: Option<u64>,
    pub entities: usize,
    pub trip_updates: usize,
    pub vehicles: usize,
    pub alerts: usize,
    /// Individual trip updates, capped for UI sanity.
    pub trips: Vec<TripUpdateView>,
    /// Live vehicles with positions, for the map.
    pub vehicle_positions: Vec<VehicleView>,
}

/// Rendered trip updates, capped for UI sanity. Kept generous: the UI
/// joins these onto departure boards and vehicle dots by trip_id, so
/// a low cap silently drops delay information.
const MAX_TRIPS_SHOWN: usize = 1000;

/// Decode a GTFS-rt protobuf payload into the overview view.
pub fn decode_rt_overview(bytes: &[u8]) -> Result<RtOverview, crate::OpError> {
    let msg = transit_rt::decode(bytes).map_err(|e| crate::OpError::new(e.to_string()))?;

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
    Ok(overview)
}
