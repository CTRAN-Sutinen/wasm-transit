//! Per-feed state and the module-level registry that owns it.
//!
//! Every op addresses a feed by its registry id — hosts never hold
//! typed handles across the boundary, which is what lets the same
//! dispatch run in the browser, under wazero, or in native tests.
//! The registry exists for the sync seam's feed-scoped validation
//! (the server's POST /edits path); every feed-side read answers
//! through the capability bus now (U6), so a feed carries nothing
//! derived.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use transit_core::Gtfs;

use crate::OpError;

/// One loaded feed: the parsed artifact, nothing derived — the map
/// draws from model documents, the day views patch their own
/// materialized days.
pub struct FeedState {
    pub gtfs: Gtfs,
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

/// Register a parsed feed; returns its id.
pub fn register_feed(gtfs: Gtfs) -> u32 {
    let id = next_feed_id();
    registry().lock().expect("feed registry poisoned").insert(
        id,
        FeedState { gtfs },
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
