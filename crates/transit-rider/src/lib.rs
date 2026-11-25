//! A rider's own documents. They never read the network or the operator's
//! day, and transit-dispatch does not depend on this crate. A watchlist
//! without tags names its version as `"version": "vN"`. Its v1 comes forward
//! through `migrate_to` (cap-migrate carries the chain), and
//! `Versioned::version_in` reads the old marker so a refusal names it.

pub mod device;
pub mod watchlist;
pub mod load;
