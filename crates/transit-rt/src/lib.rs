//! transit-rt — GTFS-realtime decoding.
//!
//! The message types in [`gtfs_rt`] are generated at build time by
//! `build.rs` from the official `gtfs-realtime.proto` (Google's spec, see
//! `proto/` in this crate). Decoding is therefore always in sync with the
//! spec file — regenerate by touching the proto.
//!
//! This crate deliberately does nothing else: turning a decoded
//! `FeedMessage` into *meaning* (e.g. "this trip is 3 minutes late at
//! stop A") is the job of the layer that also knows the static feed —
//! see transit-wasm / transit-core.

pub mod gtfs_rt {
    include!(concat!(env!("OUT_DIR"), "/transit_realtime.rs"));
}

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
