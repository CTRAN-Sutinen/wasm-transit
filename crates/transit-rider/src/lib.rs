//! transit-rider — a rider's own documents.
//!
//! The watchlist is a rider's saved stops, and the device records are
//! what ride-now keeps for its rider on one device. They are the rider's
//! and not the agency's: nothing here reads the network or the operator's
//! day, and transit-dispatch (the operator's day) does not depend on this
//! crate. cap-board reads the watchlist, and cap-migrate carries its
//! version chain.

pub mod device;
pub mod watchlist;
pub mod load;
