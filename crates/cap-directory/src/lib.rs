#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_network::Network;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct DirectoryStop {
    pub stop_id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// A stop with a parent, or with children, pools on the station board.
    pub parent: Option<String>,
    pub transfer: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct DirectoryRoute {
    pub route_id: String,
    pub name: String,
    pub long_name: Option<String>,
    pub mode: transit_network::Mode,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct StopName {
    pub stop_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct PatternStops {
    pub pattern: String,
    pub name: String,
    pub stops: Vec<StopName>,
}

/// A `walks` entry of a cap-streets `access` answer. Its path is ignored.
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
    About { network_hash: String },
    Stops {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        query: Option<String>,
        #[serde(default)]
        limit: Option<u32>,
    },
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
    StopsByIds { network_hash: String, ids: Vec<String> },
    Routes { network_hash: String },
    RouteStopSequence { network_hash: String, route_id: String },
    Focus { network_hash: String, kind: transit_network::EntityKind, id: String },
    ServiceKeys { network_hash: String },
    RouteLines {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        members: Option<Vec<String>>,
    },
    Entities {
        network_hash: String,
        kind: transit_network::EntityKind,
        #[serde(default)]
        #[ts(optional)]
        query: Option<String>,
    },
    RouteDetail { network_hash: String, route_id: String },
    StopDetail {
        network_hash: String,
        stop_id: String,
        #[serde(default)]
        #[ts(optional)]
        members: Option<Vec<String>>,
    },
    Timetable {
        network_hash: String,
        route_id: String,
        #[serde(default)]
        #[ts(optional)]
        direction: Option<String>,
        members: Vec<String>,
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

pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Stops { network_hash, query, limit } => {
            let network = network_arc(&network_hash)?;
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

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: directory has no bulk op of its own")))
}

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
