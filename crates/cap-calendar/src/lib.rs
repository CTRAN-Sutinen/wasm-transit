//! cap-calendar — the calendar capability.
//!
//! One question: **which service keys run on this date?** — often
//! several at once (a regular pattern plus an exception-added
//! service) — and its reverse, **when does one service type
//! run?**. Both answers come from the EFFECTIVE day, not the
//! calendar rules alone: a key whose every trip is skipped by
//! edits (or was never representable in the model) is not running,
//! however its calendar reads — the same honesty as the runcut
//! view's empty hole.
//!
//! The day is `materialize(net, date)`, patched with the edits in
//! force on that date — the kernel's held day
//! (`cap_abi::kernel::day`); no GTFS expansion. The domain code lives
//! in the shared transit-dispatch library; this module is the seam.
//!
//! The documents it reads are the kernel's, registered by its loads
//! and content-addressed like everywhere on the bus:
//! * **network** — the same identity cap-network's import produces
//!   and cap-analysis consumes (the module is the hash authority);
//! * **edits** — the operator edit document, wholesale-replaced by
//!   the host.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::{EDITS, NETWORK};
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
    /// The dates one service type runs on within a window — the same
    /// question `active_services` answers in the other direction
    /// ("when does 1-WKDY/31 run?"). Inclusive "YYYY-MM-DD" window;
    /// an unknown type or an inverted window answers empty.
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

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::ActiveServices { network_hash, edits_hash, date } => {
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
            Ok(Response::ActiveServices {
                services: transit_dispatch::active_services(&network, &day.trips),
            })
        }
        Request::ServiceDates { network_hash, edits_hash, service_type, from, to } => {
            // The request's own fields validate before any registry
            // lookup — a garbage date is a caller error, not a
            // missing-document error.
            let from = parse_date(&from)?;
            let to = parse_date(&to)?;
            let network = NETWORK.require(&network_hash)?;
            let records = match edits_hash.as_deref() {
                Some(hash) => EDITS.require(hash)?,
                None => Arc::new(Vec::new()),
            };
            let dates = transit_dispatch::service_dates(&network, &records, &service_type, from, to)
                .map_err(|e| format!("the network does not materialize: {e}"))?;
            Ok(Response::ServiceDates {
                dates: dates.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect(),
            })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The calendar has no bulk op of its own (its documents register on
/// the kernel's loads); the seam answers honestly instead of silently
/// ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: the calendar has no bulk op of its own")))
}

cap_abi::capability! {
    id: "calendar",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [NETWORK, EDITS],
}
