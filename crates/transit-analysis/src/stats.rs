use serde::Serialize;
#[cfg(feature = "bindings")]
use ts_rs::TS;


#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct StopStats {
    pub stop_id: String,
    pub name: String,
    pub routes: u32,
    pub departures: u32,
    pub first_arrival_secs: Option<u32>,
    pub last_arrival_secs: Option<u32>,
    pub typical_headway_secs: Option<u32>,
    pub transfer: bool,
    pub transfer_group: Option<u32>,
    /// Ordered route pairs with a usable timed connection. Zero means the flag
    /// is aspirational today.
    pub transfer_pairs: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct RouteStats {
    pub route_id: String,
    pub name: String,
    pub patterns: u32,
    pub stops: u32,
    pub trips: u32,
    pub first_start_secs: Option<u32>,
    pub last_arrival_secs: Option<u32>,
    /// Median gap between departures at the busiest stop (`headway_stop`). Null
    /// below four departures, because a median over fewer is a headway nobody
    /// runs.
    pub typical_headway_secs: Option<u32>,
    pub headway_stop: Option<String>,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
}
