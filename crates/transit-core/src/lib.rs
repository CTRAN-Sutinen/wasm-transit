//! transit-core — our own GTFS static implementation.
//!
//! Data model in [`model`], service calendar in [`calendar`], zip/CSV
//! loading in [`parse`], GTFS time handling in [`time`]. The public entry
//! point is [`Gtfs::from_zip_bytes`].

pub mod calendar;
pub mod diversion;
pub mod error;
pub mod model;
pub mod parse;
pub mod time;

pub use calendar::ServiceCalendar;
pub use diversion::{apply_diversions, affected_trips, validate_diversion, DetourPoint, Diversion};
pub use error::{GtfsError, Result};
pub use model::{route_type_name, Agency, BlockSummary, BlockTrip, Calendar, CalendarDate, Departure, FeedInfo, FeedSummary, Route, RouteSummary, ShapePoint, Stop, StopSummary, StopTime, Trip};
pub use time::{fmt_time, parse_time};

use std::collections::HashMap;

use chrono::NaiveDate;

/// A parsed GTFS static feed, ready for queries.
///
/// Id-keyed entities live in `HashMap`s (stop lookups, trip lookups);
/// `stop_times` stays a flat `Vec` because trips are processed as
/// sequences; the secondary indexes give stop → rows, trip → rows (in
/// sequence order), and block → trip ids.
#[derive(Debug)]
pub struct Gtfs {
    pub agencies: Vec<Agency>,
    pub routes: HashMap<String, Route>,
    pub stops: HashMap<String, Stop>,
    pub trips: HashMap<String, Trip>,
    pub stop_times: Vec<StopTime>,
    pub services: ServiceCalendar,
    /// stop_id → indices into [`Gtfs::stop_times`].
    stop_times_by_stop: HashMap<String, Vec<u32>>,
    /// trip_id → indices into [`Gtfs::stop_times`], sorted by
    /// `stop_sequence` (the vehicle's path through the trip).
    stop_times_by_trip: HashMap<String, Vec<u32>>,
    /// block_id → member trip ids.
    blocks: HashMap<String, Vec<String>>,
    /// shape_id → points in sequence order.
    pub shapes: HashMap<String, Vec<ShapePoint>>,
    /// `feed_info.txt` metadata, when the feed provides it.
    pub feed_info: Option<FeedInfo>,
}

impl Gtfs {
    /// Parse a feed from the bytes of a GTFS `.zip` archive.
    pub fn from_zip_bytes(bytes: &[u8]) -> Result<Gtfs> {
        parse::from_zip_bytes(bytes)
    }

    /// Build a feed from parts, wiring the secondary indexes. The zip
    /// parser is the usual producer; the network materializer in
    /// transit-ops (an authored network document expanded into the
    /// derived-feed view) is the other one.
    pub fn from_parts(
        agencies: Vec<Agency>,
        routes: HashMap<String, Route>,
        stops: HashMap<String, Stop>,
        trips: HashMap<String, Trip>,
        stop_times: Vec<StopTime>,
        services: ServiceCalendar,
        shapes: HashMap<String, Vec<ShapePoint>>,
        feed_info: Option<FeedInfo>,
    ) -> Gtfs {
        let mut gtfs = Gtfs {
            agencies,
            routes,
            stops,
            trips,
            stop_times,
            services,
            stop_times_by_stop: HashMap::new(),
            stop_times_by_trip: HashMap::new(),
            blocks: HashMap::new(),
            shapes,
            feed_info,
        };
        gtfs.rebuild_indexes();
        gtfs
    }

    /// Rebuild the secondary indexes from the current `stop_times` and
    /// `trips`. Used after diversion patching, which rewrites rows
    /// wholesale.
    pub(crate) fn rebuild_indexes(&mut self) {
        let mut stop_times_by_stop: HashMap<String, Vec<u32>> = HashMap::new();
        let mut stop_times_by_trip: HashMap<String, Vec<u32>> = HashMap::new();
        for (i, st) in self.stop_times.iter().enumerate() {
            stop_times_by_stop.entry(st.stop_id.clone()).or_default().push(i as u32);
            stop_times_by_trip.entry(st.trip_id.clone()).or_default().push(i as u32);
        }
        for indices in stop_times_by_trip.values_mut() {
            indices.sort_by_key(|&i| self.stop_times[i as usize].stop_sequence);
        }
        let mut blocks: HashMap<String, Vec<String>> = HashMap::new();
        for trip in self.trips.values() {
            if let Some(block_id) = &trip.block_id {
                blocks.entry(block_id.clone()).or_default().push(trip.trip_id.clone());
            }
        }
        self.stop_times_by_stop = stop_times_by_stop;
        self.stop_times_by_trip = stop_times_by_trip;
        self.blocks = blocks;
    }

    /// Name-sorted route list for UI pickers.
    pub fn route_summaries(&self) -> Vec<RouteSummary> {
        let mut list: Vec<RouteSummary> = self
            .routes
            .values()
            .map(|r| RouteSummary {
                route_id: r.route_id.clone(),
                name: r.display_name(),
                route_type: r.route_type_name().to_string(),
            })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.route_id.cmp(&b.route_id)));
        list
    }

    /// The stop sequence of a route (+direction), as served by its
    /// longest matching trip — the pattern the diversion editor
    /// anchors against. Trips of the route that follow other variants
    /// simply don't match the anchors and run untouched. Consecutive
    /// duplicate stops are collapsed.
    pub fn route_stop_sequence(&self, route_id: &str, direction_id: Option<u8>) -> Vec<StopSummary> {
        let mut best: Option<(&str, usize)> = None;
        for trip in self.trips.values() {
            if trip.route_id != route_id {
                continue;
            }
            if let Some(dir) = direction_id {
                if trip.direction_id != Some(dir) {
                    continue;
                }
            }
            let len = self.stop_times_by_trip.get(&trip.trip_id).map_or(0, |v| v.len());
            match best {
                // Ties break on trip id: HashMap order must not leak.
                Some((_, best_len)) if len < best_len => {}
                Some((best_id, best_len)) if len == best_len && trip.trip_id.as_str() > best_id => {}
                _ => best = Some((trip.trip_id.as_str(), len)),
            }
        }
        let Some((trip_id, _)) = best else { return Vec::new() };
        let Some(rows) = self.trip_stop_times(trip_id) else { return Vec::new() };
        let mut out: Vec<StopSummary> = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(stop) = self.stops.get(&row.stop_id) else { continue };
            if out.last().map(|s| s.stop_id.as_str()) == Some(stop.stop_id.as_str()) {
                continue;
            }
            out.push(StopSummary { stop_id: stop.stop_id.clone(), name: stop.stop_name.clone() });
        }
        out
    }

    /// Entity counts, for a quick feed overview.
    pub fn summary(&self) -> FeedSummary {
        FeedSummary {
            agencies: self.agencies.len(),
            routes: self.routes.len(),
            stops: self.stops.len(),
            trips: self.trips.len(),
            stop_times: self.stop_times.len(),
        }
    }

    /// Service keys running on `date` — often several at once
    /// (regular patterns plus exception-added services).
    pub fn active_services(&self, date: NaiveDate) -> Vec<String> {
        self.services.active_services(date)
    }

    /// All stops as a name-sorted list, for UI pickers.
    pub fn stop_summaries(&self) -> Vec<StopSummary> {
        let mut list: Vec<StopSummary> = self
            .stops
            .values()
            .map(|s| StopSummary { stop_id: s.stop_id.clone(), name: s.stop_name.clone() })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.stop_id.cmp(&b.stop_id)));
        list
    }

    /// A trip's stop times in `stop_sequence` order. (The router needs
    /// this; the index itself stays private.)
    pub fn trip_stop_times(&self, trip_id: &str) -> Option<Vec<StopTime>> {
        let indices = self.stop_times_by_trip.get(trip_id)?;
        Some(indices.iter().map(|&i| self.stop_times[i as usize].clone()).collect())
    }

    /// Upcoming departures at a stop on a given service date.
    ///
    /// `from_secs` is seconds since midnight of `date` (see [`time`]);
    /// departures at or after it are returned, sorted by time, at most
    /// `limit` of them. After-midnight trips (e.g. "25:30:00") still
    /// belong to `date`'s service day, so they naturally appear at the
    /// end of the evening's board.
    ///
    /// This is schedule-only; GTFS-rt delay data is layered on by the
    /// caller (see transit-rt). Trips from every service active on
    /// `date` are included — a date can carry multiple service keys.
    pub fn next_departures(
        &self,
        stop_id: &str,
        date: NaiveDate,
        from_secs: u32,
        limit: usize,
    ) -> Vec<Departure> {
        let Some(indices) = self.stop_times_by_stop.get(stop_id) else {
            return Vec::new();
        };

        let mut departures: Vec<Departure> = Vec::new();
        for &i in indices {
            let st = &self.stop_times[i as usize];
            let Some(departure_time) = st.departure_time else { continue };
            if departure_time < from_secs {
                continue;
            }
            let Some(trip) = self.trips.get(&st.trip_id) else { continue };
            if !self.services.is_active(&trip.service_id, date) {
                continue;
            }
            let route = self.routes.get(&trip.route_id);
            departures.push(Departure {
                stop_id: st.stop_id.clone(),
                trip_id: st.trip_id.clone(),
                route_id: trip.route_id.clone(),
                route_name: route
                    .map(|r| r.display_name())
                    .unwrap_or_else(|| trip.route_id.clone()),
                service_id: trip.service_id.clone(),
                headsign: st.stop_headsign.clone().or_else(|| trip.trip_headsign.clone()),
                block_id: trip.block_id.clone(),
                departure_time: Some(departure_time),
                departure_time_hhmmss: fmt_time(departure_time),
                stop_sequence: st.stop_sequence,
            });
        }

        departures.sort_by_key(|d| d.departure_time.unwrap_or(0));
        departures.truncate(limit);
        departures
    }

    /// All blocks in the feed, sorted by id. Empty if the feed has no
    /// `block_id` column (many don't).
    pub fn blocks(&self) -> Vec<BlockSummary> {
        let mut list: Vec<BlockSummary> = self
            .blocks
            .iter()
            .map(|(id, trips)| BlockSummary { block_id: id.clone(), trip_count: trips.len() })
            .collect();
        list.sort_by(|a, b| a.block_id.cmp(&b.block_id));
        list
    }

    /// The trips of one block, as the vehicle runs them on `date`,
    /// sorted by start time: where it starts, where it ends, and the
    /// layover between consecutive trips.
    ///
    /// The date matters in two ways: only trips whose service runs on
    /// `date` are chained, and a date with multiple active services
    /// contributes trips from all of them (each `BlockTrip` carries its
    /// `service_id` so overlapping patterns are distinguishable).
    pub fn block_trips(&self, block_id: &str, date: NaiveDate) -> Vec<BlockTrip> {
        let Some(trip_ids) = self.blocks.get(block_id) else {
            return Vec::new();
        };

        // Active trips for this date, sorted by their first departure.
        let trip_start = |trip_id: &str| -> Option<u32> {
            self.stop_times_by_trip
                .get(trip_id)?
                .iter()
                .filter_map(|&i| self.stop_times[i as usize].departure_time)
                .min()
        };
        let mut active: Vec<&Trip> = trip_ids
            .iter()
            .filter_map(|id| self.trips.get(id))
            .filter(|t| self.services.is_active(&t.service_id, date))
            .collect();
        active.sort_by_key(|t| trip_start(&t.trip_id).unwrap_or(0));

        let mut out = Vec::with_capacity(active.len());
        let mut previous_end: Option<u32> = None;
        for trip in active {
            let Some(indices) = self.stop_times_by_trip.get(&trip.trip_id) else { continue };
            let (first, last) = match (indices.first(), indices.last()) {
                (Some(&f), Some(&l)) => (&self.stop_times[f as usize], &self.stop_times[l as usize]),
                _ => continue,
            };

            let start_time = first.departure_time.or(first.arrival_time);
            let end_time = last.arrival_time.or(last.departure_time);
            let layover_secs = match (previous_end, start_time) {
                (Some(prev), Some(start)) if start >= prev => Some(start - prev),
                _ => None,
            };
            previous_end = end_time;

            out.push(BlockTrip {
                trip_id: trip.trip_id.clone(),
                route_id: trip.route_id.clone(),
                route_name: self
                    .routes
                    .get(&trip.route_id)
                    .map(|r| r.display_name())
                    .unwrap_or_else(|| trip.route_id.clone()),
                service_id: trip.service_id.clone(),
                headsign: trip.trip_headsign.clone(),
                first_stop_id: first.stop_id.clone(),
                first_stop_name: self
                    .stops
                    .get(&first.stop_id)
                    .map(|s| s.stop_name.clone())
                    .unwrap_or_else(|| first.stop_id.clone()),
                start_time,
                start_time_hhmmss: start_time.map(fmt_time).unwrap_or_default(),
                last_stop_id: last.stop_id.clone(),
                last_stop_name: self
                    .stops
                    .get(&last.stop_id)
                    .map(|s| s.stop_name.clone())
                    .unwrap_or_else(|| last.stop_id.clone()),
                end_time,
                end_time_hhmmss: end_time.map(fmt_time).unwrap_or_default(),
                layover_secs,
            });
        }
        out
    }

    /// The geographic path of a trip as (lat, lon) points: the
    /// `shapes.txt` polyline if the feed provides one, otherwise
    /// straight segments between the trip's stops (in stop_sequence
    /// order, consecutive duplicates removed).
    ///
    /// Empty if the trip is unknown or has no usable geometry.
    pub fn trip_path(&self, trip_id: &str) -> Vec<(f64, f64)> {
        let Some(trip) = self.trips.get(trip_id) else {
            return Vec::new();
        };
        if let Some(shape_id) = &trip.shape_id {
            if let Some(points) = self.shapes.get(shape_id) {
                return points
                    .iter()
                    .map(|p| (p.shape_pt_lat, p.shape_pt_lon))
                    .collect();
            }
        }

        let Some(indices) = self.stop_times_by_trip.get(trip_id) else {
            return Vec::new();
        };
        let mut path: Vec<(f64, f64)> = Vec::new();
        for &i in indices {
            let st = &self.stop_times[i as usize];
            let Some(stop) = self.stops.get(&st.stop_id) else { continue };
            let (Some(lat), Some(lon)) = (stop.stop_lat, stop.stop_lon) else {
                continue;
            };
            if path.last() != Some(&(lat, lon)) {
                path.push((lat, lon));
            }
        }
        path
    }

    /// The path of one *segment* of a trip: from one of its stops to a
    /// later one (e.g. the leg of a planned itinerary). Prefers the
    /// shapes.txt polyline sliced between the stops' nearest points,
    /// falls back to the stops between the two positions, and finally
    /// to a straight line. `(lat, lon)` points; empty if unknown.
    pub fn trip_path_segment(
        &self,
        trip_id: &str,
        from_stop_id: &str,
        to_stop_id: &str,
    ) -> Vec<(f64, f64)> {
        let coords = |stop_id: &str| {
            self.stops.get(stop_id).and_then(|s| match (s.stop_lat, s.stop_lon) {
                (Some(lat), Some(lon)) => Some((lat, lon)),
                _ => None,
            })
        };
        let (Some(from), Some(to)) = (coords(from_stop_id), coords(to_stop_id)) else {
            return Vec::new();
        };
        let Some(trip) = self.trips.get(trip_id) else {
            return Vec::new();
        };

        // Shape polyline: slice between the stops' nearest vertices.
        if let Some(shape_id) = &trip.shape_id {
            if let Some(points) = self.shapes.get(shape_id) {
                if points.len() >= 2 {
                    let i0 = nearest_shape_point(points, from);
                    let i1 = nearest_shape_point(points, to);
                    let (lo, hi) = if i0 <= i1 { (i0, i1) } else { (i1, i0) };
                    if hi > lo {
                        return points[lo..=hi]
                            .iter()
                            .map(|p| (p.shape_pt_lat, p.shape_pt_lon))
                            .collect();
                    }
                }
            }
        }

        // Fallback: the trip's own stops between the two positions.
        if let Some(indices) = self.stop_times_by_trip.get(trip_id) {
            let position = |stop_id: &str| {
                indices
                    .iter()
                    .position(|&i| self.stop_times[i as usize].stop_id == stop_id)
            };
            if let (Some(p0), Some(p1)) = (position(from_stop_id), position(to_stop_id)) {
                let (a, b) = if p0 <= p1 { (p0, p1) } else { (p1, p0) };
                let mut path: Vec<(f64, f64)> = Vec::new();
                for &i in &indices[a..=b] {
                    if let Some((lat, lon)) = coords(&self.stop_times[i as usize].stop_id) {
                        if path.last() != Some(&(lat, lon)) {
                            path.push((lat, lon));
                        }
                    }
                }
                if path.len() >= 2 {
                    return path;
                }
            }
        }

        // Last resort: straight line between the two stops.
        vec![from, to]
    }
}

/// Index of the shape vertex closest to a coordinate.
pub(crate) fn nearest_shape_point(points: &[ShapePoint], target: (f64, f64)) -> usize {
    let mut best = 0;
    let mut best_dist = f64::INFINITY;
    for (i, p) in points.iter().enumerate() {
        let dlat = p.shape_pt_lat - target.0;
        let dlon = p.shape_pt_lon - target.1;
        let d = dlat * dlat + dlon * dlon;
        if d < best_dist {
            best_dist = d;
            best = i;
        }
    }
    best
}
