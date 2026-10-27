//! transit-rt — GTFS-realtime decoding.
//!
//! The message types in [`gtfs_rt`] are generated at build time by
//! `build.rs` from the official `gtfs-realtime.proto` (Google's spec, see
//! `proto/` in this crate). Decoding is therefore always in sync with the
//! spec file — regenerate by touching the proto.
//!
//! [`overview`] turns the decoded `FeedMessage` into the flat view every
//! host shows (delays, vehicles). [`progress`] is the JOIN: realtime
//! delays against the static schedule — one trip's predicted times and
//! the ranked "what's late now" view — shared between the cap-rt
//! capability and any host that holds both documents. [`live`]
//! resolves a snapshot into THE live document (transit-live): model
//! vocabulary, so model-side consumers never re-join rt by hand.

pub mod gtfs_rt {
    include!(concat!(env!("OUT_DIR"), "/transit_realtime.rs"));
}

pub mod overview;
pub mod progress;
pub mod live;

pub use overview::{
    decode_overview, RtOverview, StopUpdateView, TripUpdateView, VehicleView,
};
pub use progress::{trip_progress, late_trips, LateTrip, ProgressStop, TripProgress};
pub use live::live_day;

use prost::Message;

#[derive(Debug, thiserror::Error)]
pub enum RtError {
    #[error("protobuf decode failed: {0}")]
    Decode(#[from] prost::DecodeError),
}

pub type Result<T> = std::result::Result<T, RtError>;

/// Decode a GTFS-rt protobuf payload (the raw bytes an agency serves,
/// typically polled every 15-30 seconds) into a [`gtfs_rt::FeedMessage`].
pub fn decode(bytes: &[u8]) -> Result<gtfs_rt::FeedMessage> {
    Ok(gtfs_rt::FeedMessage::decode(bytes)?)
}
