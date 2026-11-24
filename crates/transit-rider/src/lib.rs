//! transit-rider: a rider's own documents.
//!
//! The watchlist (a rider's saved stops, tagged at v2) and the device
//! records that ride-now keeps on one device (`recent_places`,
//! `live_feeds`, each at v1). They are the rider's, not the agency's:
//! nothing here reads the network or the operator's day, and
//! transit-dispatch does not depend on this crate. A watchlist from before
//! the tags names `"version": "vN"`; its v1 comes forward through
//! `migrate_to` (cap-migrate carries the chain), and `Versioned::version_in`
//! reads the old marker so a refusal names it.

pub mod device;
pub mod watchlist;
pub mod load;
