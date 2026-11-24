//! How the kernel loads a kind (D-239). Each library implements [`Load`]
//! for its own kind: the kind's id, its load op and code, how it reads
//! the bytes, and what the load answers. A capability names the kind's
//! type (`capability! { documents: [Notes] }`), and the kernel keeps one
//! registry for each type without knowing the kinds. So the kernel
//! depends on no domain library.
//!
//! A library reads and hashes its kind with [`crate::document`]: the
//! versioned read (gzip or plain JSON, refused in another version, read
//! one version ahead where the kind allows it) and the canonical hash.
//! Both are generic, so they are compiled once, in the library, and one
//! assembly carries one copy of each kind's reader however many of its
//! capabilities read the kind.
//!
//! Each impl's constants match the kind's line in the kind list
//! (`crates/cap-abi/src/kinds.rs`). The generator checks every line
//! against its type.

/// How a kind crosses into a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// A JSON document, under this field on the JSON lane
    /// (`{"op": "load_notes", "notes": …}`). Its identity is its
    /// canonical hash, so byte variants of one document are one document.
    Json { field: &'static str },
    /// Bytes with their own format (a zip, a protobuf, a GeoJSON layer).
    /// The bytes are the document: its identity is their content hash,
    /// and the JSON lane sends them as `bytes`, an array of numbers.
    Bytes,
}

/// How many documents of a kind an instance keeps on its own (D-70).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retain {
    /// Every document stays until the host's keep-set drops it.
    Host,
    /// A short-lived kind: the last N stay, whatever the host keeps.
    Last(usize),
}

/// A kind the kernel loads: its registry's names, its two load ops, and
/// the load itself.
pub trait Load: Sized + Send + Sync + 'static {
    /// The registry's id, as `capability! { documents }` and the
    /// manifest's `documents` name it.
    const ID: &'static str;
    /// The registry's name, as `held` and the unknown-hash error name it.
    const NAME: &'static str;
    /// The JSON op that registers it (`load_<id>`).
    const LOAD_OP: &'static str;
    /// The bulk op that registers it (256 and up: a kernel op).
    const CODE: u32;
    const FORM: Form;
    const RETAIN: Retain = Retain::Host;
    /// What the load answers beside the op: the document's hash under
    /// the kind's field (`notes_hash`), and what the host shows about it.
    type Answer: serde::Serialize;
    /// The document from bulk bytes, refused unless a write could store
    /// it, each refusal named.
    fn parse(bytes: &[u8]) -> Result<Self, String>;
    /// The document's canonical hash, for a JSON form. A byte form
    /// answers `None`: its identity is the hash of its bytes.
    fn canonical(&self) -> Option<Result<String, String>>;
    /// The load's answer for the document held under `hash`.
    fn answer(&self, hash: String) -> Self::Answer;
}
