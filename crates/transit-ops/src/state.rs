//! Per-feed state and the module-level registry that owns it.
//!
//! Today JS holds a `GtfsHandle`; with the ops layer the *module*
//! owns feed state instead: [`Request::LoadFeed`] registers a parsed
//! feed and returns its id, every other op addresses a feed by that
//! id. Hosts therefore never hold typed handles across the boundary —
//! which is what lets the same dispatch run under wasm-bindgen,
//! wasmtime, or native tests.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use chrono::NaiveDate;

use transit_core::Gtfs;
use transit_router::RouterNetwork;
use transit_streets::StreetNetwork;
use transit_sync::EditRecord;

use crate::OpError;

/// The routing networks for one service date: the effective network
/// (edits applied) and, built lazily on first comparison, the baseline
/// network (base feed) for before/after plans.
pub struct RouterCache {
    pub effective: RouterNetwork,
    pub baseline: Option<RouterNetwork>,
}

/// Everything one loaded feed carries: the immutable artifact, the
/// pushed edit document (a materialized view of the caller's
/// document), and the derived caches. See transit-wasm's docs for the
/// caching rules; the logic is unchanged, only its home moved.
pub struct FeedState {
    pub gtfs: Gtfs,
    /// Routing networks for one service date (keyed by its
    /// "YYYY-MM-DD" string).
    pub router: Option<(String, RouterCache)>,
    /// The pushed edit document (order preserved).
    pub edits: Vec<EditRecord>,
    /// Patched feed for the last date-scoped query; only Some when
    /// some in-force edit covers that date.
    pub effective: Option<(String, Gtfs)>,
}

impl FeedState {
    /// In-force diversions covering `date` (retired records are
    /// tombstones and never apply).
    pub fn applicable_diversions(&self, date: NaiveDate) -> Vec<transit_core::Diversion> {
        transit_sync::in_force_on(&self.edits, date)
    }

    pub fn invalidate(&mut self) {
        // Any edit change makes every derived structure stale.
        self.effective = None;
        self.router = None;
    }

    /// Run `f` against the effective feed for `date_str` — the base
    /// feed when no in-force edit covers the date, else the cached
    /// patched feed (rebuilt when the date changes).
    pub fn with_effective<R>(
        &mut self,
        date_str: &str,
        f: impl FnOnce(&Gtfs) -> R,
    ) -> Result<R, OpError> {
        let date = crate::parse_date(date_str)?;
        let applicable = self.applicable_diversions(date);
        if applicable.is_empty() {
            return Ok(f(&self.gtfs));
        }
        if self.effective.as_ref().is_none_or(|(key, _)| key != date_str) {
            let patched = transit_core::apply_diversions(&self.gtfs, date, &applicable)
                .map_err(|e| OpError::new(e.to_string()))?;
            self.effective = Some((date_str.to_string(), patched));
        }
        let (_, patched) = self.effective.as_ref().expect("patched feed just built");
        Ok(f(patched))
    }

    /// Make sure the router cache holds the networks for `date_str`.
    pub fn ensure_router(&mut self, date_str: &str) -> Result<(), OpError> {
        if self.router.as_ref().is_some_and(|(key, _)| key == date_str) {
            return Ok(());
        }
        let date = crate::parse_date(date_str)?;
        let net = self.with_effective(date_str, |gtfs| RouterNetwork::build(gtfs, date))?;
        self.router = Some((date_str.to_string(), RouterCache { effective: net, baseline: None }));
        Ok(())
    }
}

type Registry = BTreeMap<u32, FeedState>;

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn next_feed_id() -> u32 {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------
// Street networks: a second, parallel registry. Street data is app
// config (reference geometry for detour authoring), not feed state —
// it outlives feeds and never mixes with them.
// ---------------------------------------------------------------------

fn streets_registry() -> &'static Mutex<BTreeMap<u32, StreetNetwork>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<u32, StreetNetwork>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Register a parsed street network; returns its id.
pub fn register_streets(net: StreetNetwork) -> u32 {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    streets_registry()
        .lock()
        .expect("streets registry poisoned")
        .insert(id, net);
    id
}

/// Borrow a street network for one op. Unknown ids are an `OpError`.
pub fn with_streets<R>(
    streets_id: u32,
    f: impl FnOnce(&StreetNetwork) -> Result<R, OpError>,
) -> Result<R, OpError> {
    let registry = streets_registry()
        .lock()
        .expect("streets registry poisoned");
    let net = registry
        .get(&streets_id)
        .ok_or_else(|| OpError::new(format!("unknown streets_id {streets_id}")))?;
    f(net)
}

/// Register a parsed feed; returns its id.
pub fn register_feed(gtfs: Gtfs) -> u32 {
    let id = next_feed_id();
    registry().lock().expect("feed registry poisoned").insert(
        id,
        FeedState { gtfs, router: None, edits: Vec::new(), effective: None },
    );
    id
}

/// Release a feed (used when a republished artifact replaces the one
/// a host pinned).
pub fn drop_feed(feed_id: u32) {
    registry().lock().expect("feed registry poisoned").remove(&feed_id);
}

/// Borrow a feed's state for one op. Unknown ids are an `OpError` —
/// hosts translate that into their own error type.
pub fn with_feed<R>(
    feed_id: u32,
    f: impl FnOnce(&mut FeedState) -> Result<R, OpError>,
) -> Result<R, OpError> {
    let mut registry: MutexGuard<'_, Registry> =
        registry().lock().expect("feed registry poisoned");
    let state = registry
        .get_mut(&feed_id)
        .ok_or_else(|| OpError::new(format!("unknown feed_id {feed_id}")))?;
    f(state)
}
