//! cap-directory: what exists in the network? (OPS.md has the ops.)
//!
//! The seam for the pickers, the names join, the map's focus, and the
//! planner's lists and timetable over transit-network. These are model
//! questions, never feed reads; the feed's own line reads `load_feed` and
//! cap-gtfs. The host computes no number.
//!
//! * Stop search matches the sign's own words, never an alias: exact,
//!   then an abbreviation, then a typo (D-48).
//! * `stops_near` answers a station once, as its walking group, at its
//!   nearest member's distance. Given a point's walks (cap-streets
//!   `access`), the places a walk reaches come first by walk, then the
//!   rest by distance (D-34).
//! * Every row that names a route carries its long name.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_network::Network;

/// One stop as the directory lists it: identity, name, and location. It
/// also gives its walking-group structure: the parent flag that a station
/// board keys off, and the transfer flag that marks a designated change point.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct DirectoryStop {
    pub stop_id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// The parent station when this stop is a bay. The station board keys
    /// off this flag. A stop with a parent, or with children, pools. A plain
    /// stop stays per-stop.
    pub parent: Option<String>,
    /// A designated transfer point (authored intent). The analysis ops
    /// measure the timed connections it enables.
    pub transfer: bool,
}

/// One route as the directory lists it: identity, display name, and long
/// name beside it when it has one. It also gives its mode, from the model's
/// closed set (not GTFS's numeric codes), and its authored color when there
/// is one.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct DirectoryRoute {
    pub route_id: String,
    pub name: String,
    /// The long name ("Lincoln") beside the short one; `None` for a
    /// route without one.
    pub long_name: Option<String>,
    pub mode: transit_network::Mode,
    pub color: Option<String>,
}

/// One stop on a pattern, in order.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct StopName {
    pub stop_id: String,
    pub name: String,
}

/// One pattern of a route: its identity, its human-facing name, and its
/// ordered stops. This is the alignment of service that a diversion editor
/// anchors against.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct PatternStops {
    pub pattern: String,
    pub name: String,
    pub stops: Vec<StopName>,
}

/// One operation call. Tagged like every capability's seam.
/// A point's walk to one stop. Cap-streets `access` walks pass through as
/// they are. Their paths ride along unread.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct StopWalk {
    pub stop_id: String,
    pub walk_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_directory/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// What the network is: its name and the plan it is, as the document
    /// carries them. This is the heading a host shows over a network it holds
    /// only as bytes it never parsed.
    About { network_hash: String },
    /// The stop list, name-sorted (the pickers' shape). Every stop comes with
    /// its walking-group structure. `query` narrows it the way a picker's
    /// typing does. Every whitespace-separated term must be answered,
    /// case-insensitive. A term matches inside the name or the id ("fisher
    /// landing" finds Fisher's Landing and its bays). It matches an
    /// abbreviation on the sign ("vancouver mall" finds Van Mall). It matches
    /// within a typo of a name word (D-48; the thresholds are D-91,
    /// `transit_network::listing::name_match`). Exact matches list first, then
    /// abbreviations, then typos, each by name. `limit` truncates after the
    /// sort. `None` answers the whole list. A silently truncated directory
    /// would be a lie.
    Stops {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        query: Option<String>,
        #[serde(default)]
        limit: Option<u32>,
    },
    /// The stop list asked from a POINT: the places nearest (`lat`, `lon`), by
    /// great-circle distance in meters, with ties by id. It gives at most
    /// `limit` (default 10). With `radius_m`, none is farther than that.
    /// Without it, the answer is the nearest `limit`, however far. A place is
    /// what a rider walks to. A station's bays answer once, as the station, at
    /// the distance of its nearest member. With the point's `walks`
    /// (cap-streets
    /// `access` answers them, along the streets), the places a walk reaches
    /// come first, by walk. Each carries its `walk_secs`, the shortest to any
    /// member. The rest follow by distance (D-34).
    StopsNear {
        network_hash: String,
        lat: f64,
        lon: f64,
        #[serde(default)]
        #[ts(optional)]
        limit: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        radius_m: Option<f64>,
        #[serde(default)]
        #[ts(optional)]
        walks: Option<Vec<StopWalk>>,
    },
    /// Stops by id, in the request's order. Hosts otherwise join names by hand
    /// when a view (like rt's) carries ids that a human reads as names. Unknown
    /// ids are omitted. The caller asked for stops, and silence is the honest
    /// answer for an id the network does not carry.
    StopsByIds { network_hash: String, ids: Vec<String> },
    /// The route list, name-sorted: every route with its mode.
    Routes { network_hash: String },
    /// One route's patterns with their ordered stops. These are the model's
    /// directions. An unknown route answers empty, never an error.
    RouteStopSequence { network_hash: String, route_id: String },
    /// Where one entity is on the map: its lines and the stops it serves.
    /// Lines are pattern alignments: authored, else stop to stop. A stop is its
    /// point. A schedule is its pattern. A route, a service type, or a block is
    /// every pattern it runs. Null for an unknown id.
    Focus { network_hash: String, kind: transit_network::EntityKind, id: String },
    /// The service keys, id order: each one's name, its calendar rule
    /// as authored (weekdays, date ranges, exceptions), and the routes
    /// and trips it carries.
    ServiceKeys { network_hash: String },
    /// The network on a map: every route that draws a line (id
    /// order) with its name, its color, and the distinct lines of its
    /// patterns. Given `members` (service types), only the patterns
    /// that run trips under them draw, and a route with none is left
    /// out.
    RouteLines {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        members: Option<Vec<String>>,
    },
    /// Every entity of one kind, in id order. A filter box's typing narrows it:
    /// every term must match the id, the name, or a route's long name,
    /// case-insensitive. Each row gives what places it: a pattern's route, or a
    /// schedule's pattern and service type. It also gives the trips that a
    /// schedule runs or a block chains.
    Entities {
        network_hash: String,
        kind: transit_network::EntityKind,
        #[serde(default)]
        #[ts(optional)]
        query: Option<String>,
    },
    /// One route's composition. It gives its patterns, the stops they visit,
    /// and its schedules grouped by service type. Each schedule carries the
    /// trips it runs (the materialized day's count). It also gives its
    /// directions. Each direction has its patterns and its variants, the ways
    /// a rider rides it. Variants are patterns over the same places in order,
    /// with a station's bays as one place. Null for an unknown route.
    RouteDetail { network_hash: String, route_id: String },
    /// One stop's composition. It gives the routes whose patterns visit it, in
    /// id order. It gives the stop's record. It also gives every pattern
    /// position that visits it, with the trips each runs under `members`.
    /// Every service type applies when `members` is omitted. Null for an
    /// unknown stop.
    StopDetail {
        network_hash: String,
        stop_id: String,
        #[serde(default)]
        #[ts(optional)]
        members: Option<Vec<String>>,
    },
    /// The planner's timetable: one route, one direction, and one service
    /// combination (`members`, the types that run together; see analysis
    /// `service_groups`). A null direction means its patterns in no direction.
    /// It gives every trip its members run on the direction's patterns. There
    /// is one row per trip, by start, and one column per stop in route order.
    /// Null for an unknown route or direction, or for members that never run
    /// together on a date.
    Timetable {
        network_hash: String,
        route_id: String,
        #[serde(default)]
        #[ts(optional)]
        direction: Option<String>,
        members: Vec<String>,
        /// A loaded baseline to mark the timetable against. Trips it does not
        /// run are `added`. Its trips that this one no longer runs are
        /// `removed`.
        #[serde(default)]
        #[ts(optional)]
        compare_hash: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    About { name: String, plan: transit_network::PlanMeta },
    Stops { stops: Vec<DirectoryStop> },
    StopsNear { stops: Vec<transit_network::listing::NearbyStop> },
    StopsByIds { stops: Vec<DirectoryStop> },
    Routes { routes: Vec<DirectoryRoute> },
    RouteStopSequence { patterns: Vec<PatternStops> },
    Focus { focus: Option<transit_network::EntityFocus> },
    RouteLines { routes: Vec<transit_network::RouteLines> },
    ServiceKeys { keys: Vec<transit_network::ServiceKey> },
    Entities { rows: Vec<transit_network::EntityRow> },
    RouteDetail { detail: Option<transit_network::RouteDetail> },
    StopDetail { detail: Option<transit_network::StopDetail> },
    Timetable { timetable: Option<transit_network::Timetable> },
}

/// How many places `stops_near` answers when the request names no limit.
/// That is a screenful.
pub const STOPS_NEAR_LIMIT: u32 = 10;

fn network_arc(network_hash: &str) -> Result<Arc<Network>, String> {
    kind::<transit_network::Network>().require(network_hash)
}

fn directory_stop(network: &Network, id: &str) -> Option<DirectoryStop> {
    network.stops.get(id).map(|s| DirectoryStop {
        stop_id: id.to_string(),
        name: s.name.clone(),
        lat: s.lat,
        lon: s.lon,
        parent: s.parent.clone(),
        transfer: s.transfer,
    })
}

/// The typed dispatch. Native hosts call this directly. Failures are
/// `{"error": ...}`, and never panic. The seam is total.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Stops { network_hash, query, limit } => {
            let network = network_arc(&network_hash)?;
            // The pickers' order (exact, then abbreviation, then typo;
            // by name, then id) is the library's (D-48).
            let mut stops: Vec<DirectoryStop> = transit_network::listing::find_stops(&network, query.as_deref().unwrap_or(""))
                .iter()
                .filter_map(|id| directory_stop(&network, id))
                .collect();
            if let Some(limit) = limit {
                stops.truncate(limit as usize);
            }
            Ok(Response::Stops { stops })
        }
        Request::StopsNear { network_hash, lat, lon, limit, radius_m, walks } => {
            if !(-90.0..=90.0).contains(&lat) {
                return Err(format!("lat {lat} is not a latitude (-90..=90)"));
            }
            if !(-180.0..=180.0).contains(&lon) {
                return Err(format!("lon {lon} is not a longitude (-180..=180)"));
            }
            if radius_m.is_some_and(|r| r.is_nan() || r < 0.0) {
                return Err("radius_m must be a non-negative number of meters".into());
            }
            let network = network_arc(&network_hash)?;
            let limit = limit.unwrap_or(STOPS_NEAR_LIMIT) as usize;
            let walks: Vec<(String, u32)> = walks.unwrap_or_default().into_iter().map(|w| (w.stop_id, w.walk_secs)).collect();
            let stops = transit_network::listing::stops_near_walked(&network, lat, lon, limit, radius_m, &walks);
            Ok(Response::StopsNear { stops })
        }
        Request::StopsByIds { network_hash, ids } => {
            let network = network_arc(&network_hash)?;
            let stops = ids
                .iter()
                .filter_map(|id| directory_stop(&network, id))
                .collect();
            Ok(Response::StopsByIds { stops })
        }
        Request::About { network_hash } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::About { name: network.name.clone(), plan: network.plan.clone() })
        }
        Request::Routes { network_hash } => {
            let network = network_arc(&network_hash)?;
            let mut routes: Vec<DirectoryRoute> = network
                .routes
                .iter()
                .map(|(id, r)| DirectoryRoute {
                    route_id: id.clone(),
                    name: r.label().to_string(),
                    long_name: r.long_name.clone(),
                    mode: r.mode,
                    color: r.color.clone(),
                })
                .collect();
            routes.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.route_id.cmp(&b.route_id)));
            Ok(Response::Routes { routes })
        }
        Request::RouteStopSequence { network_hash, route_id } => {
            let network = network_arc(&network_hash)?;
            let patterns = network
                .patterns
                .iter()
                .filter(|(_, p)| p.route == route_id)
                .map(|(id, p)| PatternStops {
                    pattern: id.clone(),
                    name: p.name.clone(),
                    stops: p
                        .stops
                        .iter()
                        .map(|ps| StopName {
                            stop_id: ps.stop.clone(),
                            name: network
                                .stops
                                .get(&ps.stop)
                                .map(|s| s.name.clone())
                                .unwrap_or_else(|| ps.stop.clone()),
                        })
                        .collect(),
                })
                .collect();
            Ok(Response::RouteStopSequence { patterns })
        }
        Request::Focus { network_hash, kind, id } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::Focus { focus: transit_network::entity_focus(&network, kind, &id) })
        }
        Request::ServiceKeys { network_hash } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::ServiceKeys { keys: transit_network::service_keys(&network) })
        }
        Request::RouteLines { network_hash, members } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::RouteLines { routes: transit_network::route_lines(&network, members.as_deref()) })
        }
        Request::Entities { network_hash, kind, query } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::Entities {
                rows: transit_network::entity_rows(&network, kind, query.as_deref().unwrap_or("")),
            })
        }
        Request::RouteDetail { network_hash, route_id } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::RouteDetail { detail: transit_network::route_detail(&network, &route_id) })
        }
        Request::StopDetail { network_hash, stop_id, members } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::StopDetail { detail: transit_network::stop_detail(&network, &stop_id, members.as_deref()) })
        }
        Request::Timetable { network_hash, route_id, direction, members, compare_hash } => {
            let network = network_arc(&network_hash)?;
            let timetable = match compare_hash {
                Some(base) => transit_network::timetable_against(
                    &network,
                    &*network_arc(&base)?,
                    &route_id,
                    direction.as_deref(),
                    &members,
                ),
                None => transit_network::timetable(&network, &route_id, direction.as_deref(), &members),
            };
            Ok(Response::Timetable { timetable })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, and `{"ok": ...}` or
/// `{"error": ...}` out. Its bytes match every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The directory has no bulk op of its own. The network registers on the
/// kernel's load. The seam answers with an error, and does not silently
/// ignore a bulk op.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: directory has no bulk op of its own")))
}

/// The worked examples. The generator runs them. No module build turns
/// the feature on.
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "directory",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[("network", &[transit_network::SCHEMA_VERSION])],
}
