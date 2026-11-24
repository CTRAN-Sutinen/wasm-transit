//! A document's identity and its versioned read. This covers the content
//! hash, the canonical hash (RFC 8785, D-169, [`crate::jcs`]), and the
//! read of a versioned kind from JSON or gzipped JSON. A kind may read one
//! version ahead (D-186). These live here, beside [`crate::Versioned`].
//! Each kind's library reads and hashes its own kind in its own
//! [`crate::Load`] (D-239). One assembly carries one copy of each kind's
//! reader. The kernel re-exports them (`cap_abi::canonical_hash` and the
//! others).

use crate::jcs;

/// "sha256:<hex>" of bytes. Every module and host shares this formula.
pub fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(bytes))
}


/// A document's identity: the content hash of its canonical bytes,
/// RFC 8785 (D-169, [`jcs`]). It streams into the digest, so hashing never
/// holds a copy of the document. It is byte-identical to hashing
/// [`canonical_bytes`].
pub fn canonical_hash<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, String> {
    use sha2::{Digest, Sha256};

    struct Digesting(Sha256);
    impl jcs::Sink for Digesting {
        fn put(&mut self, bytes: &[u8]) {
            self.0.update(bytes);
        }
    }

    let mut digest = Digesting(Sha256::new());
    jcs::write(value, &mut digest).map_err(|e| format!("document does not serialize: {e}"))?;
    Ok(format!("sha256:{:x}", digest.0.finalize()))
}

/// A document's canonical bytes (RFC 8785, D-169): what a host stores
/// and serves under [`canonical_hash`].
pub fn canonical_bytes<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, String> {
    jcs::to_string(value).map_err(|e| format!("document does not serialize: {e}"))
}


/// Whether bulk bytes are a gzip stream, by its magic bytes. No JSON
/// document starts with 0x1f.
pub fn is_gzip(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1f, 0x8b])
}

/// Parse a kernel document from bulk bytes, straight into its type.
/// The bytes are the document's JSON, or that JSON gzipped. A gzipped
/// document is decoded as it parses. Its text never exists whole in the
/// module. So registration's peak is the parsed document plus the
/// compressed input, not the text beside it. Wasm memory never shrinks
/// back from a peak. The identity is the parsed document's canonical hash
/// either way, so both forms register the same document.
pub fn parse_document<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let source: Box<dyn std::io::Read + '_> = if is_gzip(bytes) { Box::new(flate2::read::GzDecoder::new(bytes)) } else { Box::new(bytes) };
    serde_json::from_reader(std::io::BufReader::with_capacity(64 * 1024, source)).map_err(|e| format!("invalid document: {e}"))
}

/// Parse a document of a versioned kind from bulk bytes (JSON or
/// gzipped JSON). It reads the version this build reads, or the next one
/// where the kind reads one version ahead (`Versioned::AHEAD`, D-186).
/// Any other version is refused, and both versions are named. An older
/// document is named with the op that brings it forward. A newer one is
/// named as written by a newer build. The document is never parsed and
/// re-emitted (docs/FORMATS.md, "Versions"). A copy with no version field
/// is the kind's untagged version. A copy read ahead is noted for the
/// kernel ([`noting_read`]).
pub fn parse_versioned<T>(bytes: &[u8]) -> Result<T, String>
where
    T: serde::de::DeserializeOwned + crate::Versioned,
{
    let (document, read) = read_versioned::<T>(bytes)?;
    if let Some(read) = read {
        NOTED_READ.with(|noted| noted.set(Some(read)));
    }
    Ok(document)
}

/// A copy read one version ahead: how it was read, its version, and the
/// canonical hash of the copy as stored (its identity on the bus).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadAhead {
    pub how: crate::Ahead,
    pub found: u32,
    pub hash: String,
}

/// [`parse_versioned`]. It also returns how the document was read: `None`
/// in this build's version, else the copy read ahead.
pub fn read_versioned<T>(bytes: &[u8]) -> Result<(T, Option<ReadAhead>), String>
where
    T: serde::de::DeserializeOwned + crate::Versioned,
{
    use crate::ReadAs;
    match parse_document::<T>(bytes) {
        Ok(document) => match crate::reads::<T>(document.schema_version())? {
            ReadAs::Current => Ok((document, None)),
            ReadAs::Ahead(how) => ahead(parse_value(bytes)?, how),
        },
        // The typed read failed. An older or newer shape shows it by its
        // version. A document in this version is only malformed.
        Err(typed) => match parse_value(bytes) {
            Ok(value) => match crate::reads::<T>(version_in::<T>(&value))? {
                ReadAs::Current => Err(typed),
                ReadAs::Ahead(how) => ahead(value, how),
            },
            Err(_) => Err(typed),
        },
    }
}

/// A document as a JSON value ([`parse_document`]). It is a plain
/// function. It is compiled once, here, not in each kind's library that
/// reads through [`read_versioned`].
pub fn parse_value(bytes: &[u8]) -> Result<serde_json::Value, String> {
    parse_document(bytes)
}

/// A JSON value's canonical hash ([`canonical_hash`]), compiled once,
/// here, as [`parse_value`] is.
pub fn value_hash(value: &serde_json::Value) -> Result<String, String> {
    canonical_hash(value)
}

/// A copy in the next version, read through its kind's step down.
fn ahead<T>(value: serde_json::Value, how: crate::Ahead) -> Result<(T, Option<ReadAhead>), String>
where
    T: serde::de::DeserializeOwned + crate::Versioned,
{
    let found = version_in::<T>(&value);
    let hash = value_hash(&value)?;
    let down = T::step_down(value)?;
    let document = serde_json::from_value(down).map_err(|e| format!("invalid document after the step down from v{found}: {e}"))?;
    Ok((document, Some(ReadAhead { how, found, hash })))
}

thread_local! {
    /// The copy [`parse_versioned`] read ahead while [`noting_read`]
    /// runs.
    static NOTED_READ: std::cell::Cell<Option<ReadAhead>> = const { std::cell::Cell::new(None) };
}

/// Run `read`. It returns the answer beside the copy it read one version
/// ahead, if any. The kernel's load uses it. A copy read ahead is held
/// under its stored identity. A copy read through a read-only step refuses
/// a write (the kernel's `Kind::require_writable`).
pub fn noting_read<R>(read: impl FnOnce() -> R) -> (R, Option<ReadAhead>) {
    NOTED_READ.with(|noted| noted.set(None));
    let answer = read();
    (answer, NOTED_READ.with(|noted| noted.take()))
}

/// [`parse_versioned`] over a value already parsed (a request field).
pub fn versioned<T>(value: serde_json::Value) -> Result<T, String>
where
    T: serde::de::DeserializeOwned + crate::Versioned,
{
    read_value::<T>(value).map(|(document, _)| document)
}

pub fn read_value<T>(value: serde_json::Value) -> Result<(T, Option<ReadAhead>), String>
where
    T: serde::de::DeserializeOwned + crate::Versioned,
{
    match crate::reads::<T>(version_in::<T>(&value))? {
        crate::ReadAs::Current => {
            serde_json::from_value(value).map(|document| (document, None)).map_err(|e| format!("invalid document: {e}"))
        }
        crate::ReadAs::Ahead(how) => ahead(value, how),
    }
}


/// The version a copy of `T`'s kind carries in its version field.
pub fn version_in<T: crate::Versioned>(value: &serde_json::Value) -> u32 {
    T::version_in(value)
}


