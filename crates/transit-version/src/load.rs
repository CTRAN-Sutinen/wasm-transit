//! Each library implements [`Load`] for its own kind, so the kernel keeps one
//! registry for each type without knowing the kinds, and depends on no domain
//! library (D-239). The read and the hash in [`crate::document`] are
//! generic, so they compile once, in the library. One assembly then carries
//! one copy of each kind's reader however many capabilities read the kind.
//! The generator checks each impl's constants against the kind's line in
//! `crates/cap-abi/src/kinds.rs`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// A JSON document under this field of the JSON entry point, as in
    /// `{"op": "load_notes", "notes": …}`. Its identity is its canonical
    /// hash, so byte variants of one document are one document.
    Json { field: &'static str },
    /// Bytes with their own format (a zip, a protobuf, a GeoJSON layer).
    /// Their content hash is the identity. The JSON entry point takes them as
    /// `bytes`, an array of numbers.
    Bytes,
}

/// How long a module keeps a document of the kind (D-70).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retain {
    /// Every document stays until the host's keep list drops it.
    Host,
    /// The last N stay, whatever the host keeps.
    Last(usize),
}

pub trait Load: Sized + Send + Sync + 'static {
    const ID: &'static str;
    const NAME: &'static str;
    const LOAD_OP: &'static str;
    /// The bytes op code. 256 and up are kernel ops.
    const CODE: u32;
    const FORM: Form;
    const RETAIN: Retain = Retain::Host;
    type Answer: serde::Serialize;
    /// Refuses a document that a write could not store.
    fn parse(bytes: &[u8]) -> Result<Self, String>;
    /// `None` for a byte form, whose identity is the hash of its bytes.
    fn canonical(&self) -> Option<Result<String, String>>;
    fn answer(&self, hash: String) -> Self::Answer;
}
