//! The GTFS static data model.
//!
//! This is a pragmatic subset of the spec, not the whole thing: the files a
//! departure-board / trip-planner core actually needs. Each struct maps to
//! one GTFS CSV file and is deserialized directly by serde + the `csv`
//! crate. Column names in GTFS are already `snake_case`, so field names
//! match the headers one-to-one.
//!
//! Two conventions matter when reading this:
//!
//! * GTFS uses `0`/`1` where Rust wants `bool` — `de_bool_01` handles that.
//! * GTFS writes dates as `YYYYMMDD` strings — `de_date` parses those into
//!   `chrono::NaiveDate`.

use ts_rs::TS;
use chrono::{NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

/// Deserialize a GTFS `0`/`1` column into a Rust `bool`.
fn de_bool_01<'de, D>(d: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match u8::deserialize(d)? {
        1 => Ok(true),
        0 => Ok(false),
        other => Err(serde::de::Error::custom(format!(
            "expected 0 or 1, got {other}"
        ))),
    }
}

/// Deserialize a GTFS `YYYYMMDD` date column into a `NaiveDate`.
fn de_date<'de, D>(d: D) -> Result<NaiveDate, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(d)?;
    NaiveDate::parse_from_str(raw.trim(), "%Y%m%d")
        .map_err(|e| serde::de::Error::custom(format!("invalid date {raw:?}: {e}")))
}

/// `agency.txt`
#[derive(Debug, Clone, Deserialize)]
pub struct Agency {
    #[serde(default)]
    pub agency_id: Option<String>,
    pub agency_name: String,
    pub agency_url: String,
    pub agency_timezone: String,
}

/// `routes.txt`
#[derive(Debug, Clone, Deserialize)]
pub struct Route {
    pub route_id: String,
    #[serde(default)]
    pub agency_id: Option<String>,
    #[serde(default)]
    pub route_short_name: Option<String>,
    #[serde(default)]
    pub route_long_name: Option<String>,
    pub route_type: u8,
    #[serde(default)]
    pub route_color: Option<String>,
    #[serde(default)]
    pub route_text_color: Option<String>,
}

impl Route {
    /// Best human-readable name: short name if present, else long name.
    pub fn display_name(&self) -> String {
        self.route_short_name
            .clone()
            .or_else(|| self.route_long_name.clone())
            .unwrap_or_else(|| self.route_id.clone())
    }

    pub fn route_type_name(&self) -> &'static str {
        route_type_name(self.route_type)
    }
}

pub fn route_type_name(t: u8) -> &'static str {
    match t {
        0 => "tram",
        1 => "subway",
        2 => "rail",
        3 => "bus",
        4 => "ferry",
        5 => "cable tram",
        6 => "aerial lift",
        7 => "funicular",
        11 => "trolleybus",
        12 => "monorail",
        _ => "other",
    }
}

/// `stops.txt`
#[derive(Debug, Clone, Deserialize)]
pub struct Stop {
    pub stop_id: String,
    #[serde(default)]
    pub stop_code: Option<String>,
    pub stop_name: String,
    #[serde(default)]
    pub stop_lat: Option<f64>,
    #[serde(default)]
    pub stop_lon: Option<f64>,
    /// 0 = stop, 1 = station, 2 = entrance, 3..4 = other. Raw, per spec.
    #[serde(default)]
    pub location_type: Option<u8>,
    #[serde(default)]
    pub parent_station: Option<String>,
}

/// `trips.txt`
#[derive(Debug, Clone, Deserialize)]
pub struct Trip {
    pub route_id: String,
    pub service_id: String,
    pub trip_id: String,
    #[serde(default)]
    pub trip_headsign: Option<String>,
    #[serde(default)]
    pub direction_id: Option<u8>,
    /// Trips sharing a block_id are run by the same vehicle, in
    /// sequence. Optional — many feeds omit it entirely.
    #[serde(default)]
    pub block_id: Option<String>,
    /// Path geometry from shapes.txt, if the feed provides it.
    #[serde(default)]
    pub shape_id: Option<String>,
}

/// `calendar.txt` — a weekly service pattern valid within a date range.
#[derive(Debug, Clone, Deserialize)]
pub struct Calendar {
    pub service_id: String,
    #[serde(deserialize_with = "de_bool_01")]
    pub monday: bool,
    #[serde(deserialize_with = "de_bool_01")]
    pub tuesday: bool,
    #[serde(deserialize_with = "de_bool_01")]
    pub wednesday: bool,
    #[serde(deserialize_with = "de_bool_01")]
    pub thursday: bool,
    #[serde(deserialize_with = "de_bool_01")]
    pub friday: bool,
    #[serde(deserialize_with = "de_bool_01")]
    pub saturday: bool,
    #[serde(deserialize_with = "de_bool_01")]
    pub sunday: bool,
    #[serde(deserialize_with = "de_date")]
    pub start_date: NaiveDate,
    #[serde(deserialize_with = "de_date")]
    pub end_date: NaiveDate,
}

impl Calendar {
    pub fn runs_on(&self, weekday: Weekday) -> bool {
        match weekday {
            Weekday::Mon => self.monday,
            Weekday::Tue => self.tuesday,
            Weekday::Wed => self.wednesday,
            Weekday::Thu => self.thursday,
            Weekday::Fri => self.friday,
            Weekday::Sat => self.saturday,
            Weekday::Sun => self.sunday,
        }
    }
}

/// `calendar_dates.txt` — per-date exceptions layered on top of
/// `calendar.txt`. `exception_type`: 1 = service added, 2 = service removed.
#[derive(Debug, Clone, Deserialize)]
pub struct CalendarDate {
    pub service_id: String,
    #[serde(deserialize_with = "de_date")]
    pub date: NaiveDate,
    pub exception_type: u8,
}

/// `stop_times.txt`, already parsed: times converted to seconds since
/// service-day midnight (`None` where the feed leaves them empty).
#[derive(Debug, Clone)]
pub struct StopTime {
    pub trip_id: String,
    pub stop_id: String,
    pub stop_sequence: u32,
    pub arrival_time: Option<u32>,
    pub departure_time: Option<u32>,
    pub stop_headsign: Option<String>,
}

/// `shapes.txt` — one row per point along a vehicle's path. Points of one
/// shape are grouped and sorted by sequence in [`crate::Gtfs::shapes`].
#[derive(Debug, Clone, Deserialize)]
pub struct ShapePoint {
    pub shape_id: String,
    pub shape_pt_lat: f64,
    pub shape_pt_lon: f64,
    pub shape_pt_sequence: u32,
    #[serde(default)]
    pub shape_dist_traveled: Option<f64>,
}

/// `transfers.txt` — one row per declared transfer between stops.
/// Kept faithfully at the codec layer (route pairs, timed windows);
/// the authored model derives its transfer-point flag from these.
#[derive(Debug, Clone, Deserialize)]
pub struct Transfer {
    pub from_stop_id: String,
    pub to_stop_id: String,
    #[serde(default)]
    pub transfer_type: Option<u8>,
    #[serde(default)]
    pub min_transfer_time: Option<u32>,
}

/// One upcoming departure at a stop, joined across stop_times → trips →
/// routes. Returned by [`crate::Gtfs::next_departures`].
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Departure {
    pub stop_id: String,
    pub trip_id: String,
    pub route_id: String,
    pub route_name: String,
    /// The service key (service_id) this trip runs under. The same date
    /// can have several active services.
    pub service_id: String,
    /// Effective headsign: stop-level overrides trip-level per the spec.
    pub headsign: Option<String>,
    /// The vehicle block this trip belongs to, if the feed provides it.
    pub block_id: Option<String>,
    pub departure_time: Option<u32>,
    pub departure_time_hhmmss: String,
    pub stop_sequence: u32,
}

/// One block: the vehicle's trip chain. Listed by [`crate::Gtfs::blocks`].
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct BlockSummary {
    pub block_id: String,
    pub trip_count: usize,
}

/// One trip within a block, as returned by
/// [`crate::Gtfs::block_trips`]: where the vehicle starts, where it ends,
/// and how long it waits between the previous trip and this one.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct BlockTrip {
    pub trip_id: String,
    pub route_id: String,
    pub route_name: String,
    /// The service key (service_id) this trip runs under. A date with
    /// several active services chains all of their trips into the
    /// block, sorted by time; this says which service produced which.
    pub service_id: String,
    pub headsign: Option<String>,
    pub first_stop_id: String,
    pub first_stop_name: String,
    pub start_time: Option<u32>,
    pub start_time_hhmmss: String,
    pub last_stop_id: String,
    pub last_stop_name: String,
    pub end_time: Option<u32>,
    pub end_time_hhmmss: String,
    /// Seconds between the previous trip's end and this trip's start.
    /// `None` for the first trip (or when times are unknown/overlapping).
    pub layover_secs: Option<u32>,
}

/// High-level feed statistics (what the demo page shows first).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FeedSummary {
    pub agencies: usize,
    pub routes: usize,
    pub stops: usize,
    pub trips: usize,
    pub stop_times: usize,
}

/// Minimal stop info for UI lists.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopSummary {
    pub stop_id: String,
    pub name: String,
}

/// Minimal route info for UI pickers.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteSummary {
    pub route_id: String,
    pub name: String,
    pub route_type: String,
}

/// `feed_info.txt` — feed-level metadata. All fields are optional in
/// practice (agencies are inconsistent), but `feed_version` is the
/// one that matters: it identifies the artifact a manifest points at,
/// alongside the content hash.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FeedInfo {
    #[serde(default)]
    pub feed_id: Option<String>,
    #[serde(default)]
    pub feed_version: Option<String>,
    #[serde(default)]
    pub feed_publisher_name: Option<String>,
    #[serde(default)]
    pub feed_publisher_url: Option<String>,
    #[serde(default)]
    pub feed_lang: Option<String>,
}
