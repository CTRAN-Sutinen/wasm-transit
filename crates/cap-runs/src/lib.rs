//! cap-runs wraps transit-runs' runs cut, its two steps, and the judging of
//! runs that anyone composed (`OPS.md` has the ops). It reads the
//! same cached day as cap-runcut and cap-board. The runcut answers what a
//! vehicle does, and this capability answers what a person does.

#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_runs/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Cut {
        network_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        edits_hash: Option<String>,
        date: String,
        work_rules_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        scheduling_constraints_hash: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        streets_hash: Option<String>,
    },
    Pieces {
        network_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        edits_hash: Option<String>,
        date: String,
        work_rules_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        scheduling_constraints_hash: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        streets_hash: Option<String>,
    },
    Compose {
        network_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        edits_hash: Option<String>,
        date: String,
        work_rules_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        scheduling_constraints_hash: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        streets_hash: Option<String>,
        pieces: Vec<transit_runs::PieceRef>,
    },
    Judge {
        network_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        edits_hash: Option<String>,
        date: String,
        work_rules_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        scheduling_constraints_hash: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        streets_hash: Option<String>,
        runs: Vec<transit_runs::RunRef>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_runs/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Cut { cut: transit_runs::Cut },
    Pieces { pieces: transit_runs::Pieces },
    Compose { composed: transit_runs::RunSet },
    Judge { judged: transit_runs::RunSet },
}

struct Loaded {
    date: String,
    weekday: transit_runs::Day,
    network: std::sync::Arc<transit_network::Network>,
    rules: std::sync::Arc<transit_runs::WorkRules>,
    constraints: Option<std::sync::Arc<transit_runs::SchedulingConstraints>>,
    day: std::sync::Arc<transit_dispatch::Day>,
    graph: Option<std::sync::Arc<transit_streets::StreetNetwork>>,
}

/// The streets, for the drives to and from the garage.
struct Graph<'a>(&'a transit_streets::StreetNetwork);

impl transit_runs::Roads for Graph<'_> {
    fn meters(&self, from: (f64, f64), to: (f64, f64)) -> Option<f64> {
        let (a, b) = (self.0.snap(from.0, from.1)?, self.0.snap(to.0, to.1)?);
        self.0.route_with_length(a, b).map(|r| r.1)
    }
}

impl Loaded {
    fn blocks(&self) -> transit_runs::DayBlocks {
        let graph = self.graph.as_deref().map(Graph);
        transit_runs::DayBlocks::new(
            &self.network,
            &self.day.trips,
            self.constraints.as_deref(),
            graph.as_ref().map(|g| g as &dyn transit_runs::Roads),
        )
    }
}

fn load(
    network_hash: &str,
    edits_hash: Option<&str>,
    date: String,
    work_rules_hash: &str,
    scheduling_constraints_hash: Option<&str>,
    streets_hash: Option<&str>,
) -> Result<Loaded, cap_abi::Refusal> {
    let on = transit_dispatch::day_date(&date)?;
    let network = kind::<transit_network::Network>().require(network_hash)?;
    let rules = kind::<transit_runs::WorkRules>().require(work_rules_hash)?;
    let constraints = scheduling_constraints_hash
        .map(|h| kind::<transit_runs::SchedulingConstraints>().require(h))
        .transpose()?;
    let day = day(network_hash, edits_hash, &date)?;
    let graph = streets_hash
        .map(|h| {
            let streets = kind::<transit_streets::Streets>().require(h)?;
            cap_abi::kernel::derive(&cap_abi::kernel::STREET_GRAPH, h, &[h], || {
                Ok::<_, String>(transit_streets::StreetNetwork::build(&streets))
            })
        })
        .transpose()?;
    Ok(Loaded {
        weekday: on.weekday().into(),
        date,
        network,
        rules,
        constraints,
        day,
        graph,
    })
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::Cut {
            network_hash,
            edits_hash,
            date,
            work_rules_hash,
            scheduling_constraints_hash,
            streets_hash,
        } => {
            let l = load(
                &network_hash,
                edits_hash.as_deref(),
                date,
                &work_rules_hash,
                scheduling_constraints_hash.as_deref(),
                streets_hash.as_deref(),
            )?;
            let places = transit_runs::NetworkPlaces::new(&l.network, &l.day.trips);
            let cut = transit_runs::cut_runs(
                &l.blocks(),
                &l.rules,
                l.constraints.as_deref(),
                &l.date,
                l.weekday,
                &places,
            );
            Ok(Response::Cut { cut })
        }
        Request::Pieces {
            network_hash,
            edits_hash,
            date,
            work_rules_hash,
            scheduling_constraints_hash,
            streets_hash,
        } => {
            let l = load(
                &network_hash,
                edits_hash.as_deref(),
                date,
                &work_rules_hash,
                scheduling_constraints_hash.as_deref(),
                streets_hash.as_deref(),
            )?;
            let places = transit_runs::NetworkPlaces::new(&l.network, &l.day.trips);
            let pieces = transit_runs::cut_pieces(
                &l.blocks(),
                &l.rules,
                l.constraints.as_deref(),
                &l.date,
                l.weekday,
                &places,
            );
            Ok(Response::Pieces { pieces })
        }
        Request::Compose {
            network_hash,
            edits_hash,
            date,
            work_rules_hash,
            scheduling_constraints_hash,
            streets_hash,
            pieces,
        } => {
            let l = load(
                &network_hash,
                edits_hash.as_deref(),
                date,
                &work_rules_hash,
                scheduling_constraints_hash.as_deref(),
                streets_hash.as_deref(),
            )?;
            let places = transit_runs::NetworkPlaces::new(&l.network, &l.day.trips);
            let blocks = l.blocks();
            let composed =
                transit_runs::compose_refs(&blocks, &pieces, &l.rules, &l.date, l.weekday, &places)
                    .map_err(cap_abi::Refusal::from)?;
            Ok(Response::Compose { composed })
        }
        Request::Judge {
            network_hash,
            edits_hash,
            date,
            work_rules_hash,
            scheduling_constraints_hash,
            streets_hash,
            runs,
        } => {
            let l = load(
                &network_hash,
                edits_hash.as_deref(),
                date,
                &work_rules_hash,
                scheduling_constraints_hash.as_deref(),
                streets_hash.as_deref(),
            )?;
            let places = transit_runs::NetworkPlaces::new(&l.network, &l.day.trips);
            let blocks = l.blocks();
            let judged = transit_runs::judge_runs(
                &blocks,
                &runs,
                &l.rules,
                l.constraints.as_deref(),
                &l.date,
                l.weekday,
                &places,
            )
            .map_err(cap_abi::Refusal::from)?;
            Ok(Response::Judge { judged })
        }
    }
}

fn day(
    network_hash: &str,
    edits_hash: Option<&str>,
    date: &str,
) -> Result<std::sync::Arc<transit_dispatch::Day>, cap_abi::Refusal> {
    let on = transit_dispatch::day_date(date)?;
    let network = kind::<transit_network::Network>().require(network_hash)?;
    let edits = edits_hash
        .map(|hash| kind::<transit_sync::EditDocument>().require(hash))
        .transpose()?;
    let records = edits
        .as_deref()
        .map(|d| d.records.as_slice())
        .unwrap_or(&[]);
    let inputs: Vec<&str> = std::iter::once(network_hash).chain(edits_hash).collect();
    let key = transit_dispatch::day_key(network_hash, edits_hash, date);
    cap_abi::kernel::derive(&cap_abi::kernel::DAY, &key, &inputs, || {
        transit_dispatch::day_of(&network, records, on)
    })
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<(), String>(Err(format!(
        "unknown bulk op {op}: runs has no bulk op of its own"
    )))
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "runs",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_sync::EditDocument, transit_runs::WorkRules, transit_runs::SchedulingConstraints, transit_streets::Streets],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("operator_edits", &[transit_sync::SCHEMA_VERSION]),
        ("work_rules", &[transit_runs::SCHEMA_VERSION]),
        ("scheduling_constraints", &[transit_runs::CONSTRAINTS_VERSION]),
        ("streets", &[1]),
    ],
}
