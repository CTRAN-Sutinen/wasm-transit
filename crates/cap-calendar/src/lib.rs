//! cap-calendar: which service keys run on a date, and its reverse, on
//! which dates one service type runs (OPS.md has the ops).
//!
//! Both answers read the effective day, not the calendar rules. The day is
//! the network materialized for the date and patched by the edits in force
//! (the kernel's held day, `transit_dispatch::day_of` under
//! `cap_abi::kernel::DAY`). A key whose every trip is skipped or cannot be
//! represented is not running, whatever its calendar says. The runcut
//! view's hole keeps the same rule. It reads the same documents and day as
//! cap-board. There is no GTFS expansion here. The arch gate forbids a
//! GTFS layer.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_calendar/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// The service keys running on a date: the distinct service types
    /// the effective day's trips run under, sorted. Schedule-only
    /// when `edits_hash` is omitted.
    ActiveServices {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
    },
    /// The dates one service type runs on within a window. This is the
    /// same question `active_services` answers in the other direction
    /// ("when does 1-WKDY/31 run?"). Inclusive "YYYY-MM-DD" window.
    /// An unknown type or an inverted window answers empty.
    /// Schedule-only when `edits_hash` is omitted.
    ServiceDates {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        service_type: String,
        from: String,
        to: String,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_calendar/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    ActiveServices { services: Vec<String> },
    ServiceDates { dates: Vec<String> },
}

fn parse_date(date: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| format!("invalid date {date:?}: {e}"))
}

/// The typed dispatch. Native hosts call this directly. Failures are
/// `{"error": ...}`. It never panics: a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::ActiveServices { network_hash, edits_hash, date } => {
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            Ok(Response::ActiveServices {
                services: transit_dispatch::active_services(&network, &day.trips),
            })
        }
        Request::ServiceDates { network_hash, edits_hash, service_type, from, to } => {
            // Validate the request's own fields before any registry
            // lookup. A garbage date is a caller error, not a
            // missing-document error.
            let from = parse_date(&from)?;
            let to = parse_date(&to)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let records = match edits_hash.as_deref() {
                Some(hash) => kind::<transit_sync::EditDocument>().require(hash)?,
                None => Arc::new(Default::default()),
            };
            let dates = transit_dispatch::service_dates(&network, &records.records, &service_type, from, to)
                .map_err(|e| format!("the network does not materialize: {e}"))?;
            Ok(Response::ServiceDates {
                dates: dates.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect(),
            })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` or `{"error": ...}` out.
/// The bytes match every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The calendar has no bulk op of its own. Its documents register on the
/// kernel's loads. The seam answers with an error instead of silently
/// ignoring the bulk bytes.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: the calendar has no bulk op of its own")))
}

/// The worked examples. The generator runs them. No module build turns
/// the feature on.
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

/// The day for (network, edits, date). The kernel's derived registry holds
/// it under `transit_dispatch::day_key`. It holds the effective trips and,
/// when edits are in force, what they took away. The date is read first,
/// so a date that does not parse is the caller's error.
fn day(network_hash: &str, edits_hash: Option<&str>, date: &str) -> Result<std::sync::Arc<transit_dispatch::Day>, String> {
    let on = transit_dispatch::day_date(date)?;
    let network = kind::<transit_network::Network>().require(network_hash)?;
    let edits = edits_hash.map(|hash| kind::<transit_sync::EditDocument>().require(hash)).transpose()?;
    let records = edits.as_deref().map(|d| d.records.as_slice()).unwrap_or(&[]);
    let inputs: Vec<&str> = std::iter::once(network_hash).chain(edits_hash).collect();
    let key = transit_dispatch::day_key(network_hash, edits_hash, date);
    cap_abi::kernel::derive(&cap_abi::kernel::DAY, &key, &inputs, || transit_dispatch::day_of(&network, records, on))
}
