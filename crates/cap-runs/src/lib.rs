//! cap-runs wraps transit-runs' runs cut (`OPS.md` has the op). It reads the
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
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_runs/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Cut { cut: transit_runs::Cut },
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::Cut { network_hash, edits_hash, date, work_rules_hash, scheduling_constraints_hash } => {
            let on = transit_dispatch::day_date(&date)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let rules = kind::<transit_runs::WorkRules>().require(&work_rules_hash)?;
            let constraints =
                scheduling_constraints_hash.as_deref().map(|h| kind::<transit_runs::SchedulingConstraints>().require(h)).transpose()?;
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let places = transit_runs::NetworkPlaces::new(&network, &day.trips);
            let cut = transit_runs::cut_runs(&network, &day.trips, &rules, constraints.as_deref(), &date, on.weekday().into(), &places);
            Ok(Response::Cut { cut })
        }
    }
}

fn day(network_hash: &str, edits_hash: Option<&str>, date: &str) -> Result<std::sync::Arc<transit_dispatch::Day>, cap_abi::Refusal> {
    let on = transit_dispatch::day_date(date)?;
    let network = kind::<transit_network::Network>().require(network_hash)?;
    let edits = edits_hash.map(|hash| kind::<transit_sync::EditDocument>().require(hash)).transpose()?;
    let records = edits.as_deref().map(|d| d.records.as_slice()).unwrap_or(&[]);
    let inputs: Vec<&str> = std::iter::once(network_hash).chain(edits_hash).collect();
    let key = transit_dispatch::day_key(network_hash, edits_hash, date);
    cap_abi::kernel::derive(&cap_abi::kernel::DAY, &key, &inputs, || transit_dispatch::day_of(&network, records, on))
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<(), String>(Err(format!("unknown bulk op {op}: runs has no bulk op of its own")))
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "runs",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_sync::EditDocument, transit_runs::WorkRules, transit_runs::SchedulingConstraints],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("operator_edits", &[transit_sync::SCHEMA_VERSION]),
        ("work_rules", &[transit_runs::SCHEMA_VERSION]),
        ("scheduling_constraints", &[transit_runs::CONSTRAINTS_VERSION]),
    ],
}
