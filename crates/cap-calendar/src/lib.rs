#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_calendar/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    ActiveServices {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
    },
    ServiceDates {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        service_type: String,
        from: String,
        to: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_calendar/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    ActiveServices { services: Vec<String> },
    ServiceDates { dates: Vec<String> },
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::ActiveServices {
            network_hash,
            edits_hash,
            date,
        } => {
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            Ok(Response::ActiveServices {
                services: transit_dispatch::active_services(&network, &day.trips),
            })
        }
        Request::ServiceDates {
            network_hash,
            edits_hash,
            service_type,
            from,
            to,
        } => {
            // Check the request's own fields first, so a garbage date is a
            // caller error and not a missing-document error.
            let from = transit_date::Date::read(&from)?;
            let to = transit_date::Date::read(&to)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let records = match edits_hash.as_deref() {
                Some(hash) => kind::<transit_sync::EditDocument>().require(hash)?,
                None => Arc::new(Default::default()),
            };
            let dates = transit_dispatch::service_dates(
                &network,
                &records.records,
                &service_type,
                from,
                to,
            )
            .map_err(|e| format!("the network does not materialize: {e}"))?;
            Ok(Response::ServiceDates {
                dates: dates.iter().map(|d| d.to_string()).collect(),
            })
        }
    }
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<(), String>(Err(format!(
        "unknown bulk op {op}: the calendar has no bulk op of its own"
    )))
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "calendar",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_sync::EditDocument],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("operator_edits", &[transit_sync::SCHEMA_VERSION]),
    ],
}

/// Cached under `transit_dispatch::day_key`. The date parses first, so a bad
/// date is the caller's error.
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
