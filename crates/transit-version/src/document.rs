//! Identity and the versioned read of a document live here, beside
//! [`crate::Versioned`]. Each kind's library reads and hashes its own kind
//! in its [`crate::Load`] (D-239). The kernel re-exports these functions
//! (`cap_abi::canonical_hash` and the others).

use crate::jcs;

pub fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(bytes))
}


/// Streams the canonical bytes (RFC 8785, D-169, [`jcs`]) into the digest, so
/// hashing never holds a copy of the document. The result equals hashing
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

pub fn canonical_bytes<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, String> {
    jcs::to_string(value).map_err(|e| format!("document does not serialize: {e}"))
}


/// No JSON document starts with 0x1f.
pub fn is_gzip(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1f, 0x8b])
}

/// A gzipped document is decoded as it parses, so its text never exists whole
/// in the module. Wasm memory never shrinks back from a peak, so registration
/// peaks at the parsed document plus the compressed input. The identity is
/// the canonical hash either way, so both forms register the same document.
pub fn parse_document<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let source: Box<dyn std::io::Read + '_> = if is_gzip(bytes) { Box::new(flate2::read::GzDecoder::new(bytes)) } else { Box::new(bytes) };
    serde_json::from_reader(std::io::BufReader::with_capacity(64 * 1024, source)).map_err(|e| format!("invalid document: {e}"))
}

/// Reads the build's version, or the next one where the kind reads ahead
/// (`Versioned::AHEAD`, D-186). A copy read ahead is noted for
/// [`noting_read`]. The document is never parsed and re-emitted
/// (docs/FORMATS.md, "Versions").
pub fn parse_versioned<T>(bytes: &[u8]) -> Result<T, crate::Refusal>
where
    T: serde::de::DeserializeOwned + crate::Versioned,
{
    let (document, read) = read_versioned::<T>(bytes)?;
    if let Some(read) = read {
        NOTED_READ.with(|noted| noted.set(Some(read)));
    }
    Ok(document)
}

/// `hash` is the canonical hash of the copy as stored, which is its identity
/// on the bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadAhead {
    pub how: crate::Ahead,
    pub found: u32,
    pub hash: String,
}

pub fn read_versioned<T>(bytes: &[u8]) -> Result<(T, Option<ReadAhead>), crate::Refusal>
where
    T: serde::de::DeserializeOwned + crate::Versioned,
{
    use crate::ReadAs;
    match parse_document::<T>(bytes) {
        Ok(document) => match crate::reads::<T>(document.schema_version())? {
            ReadAs::Current => Ok((document, None)),
            ReadAs::Ahead(how) => ahead(parse_value(bytes)?, how),
        },
        // An older or newer shape fails the typed read and shows its
        // version. A document in this version is only malformed.
        Err(typed) => match parse_value(bytes) {
            Ok(value) => match crate::reads::<T>(version_in::<T>(&value))? {
                ReadAs::Current => Err(typed.into()),
                ReadAs::Ahead(how) => ahead(value, how),
            },
            Err(_) => Err(typed.into()),
        },
    }
}

/// A plain function, so it compiles once, here, and not in each kind's
/// library that reads through [`read_versioned`].
pub fn parse_value(bytes: &[u8]) -> Result<serde_json::Value, String> {
    parse_document(bytes)
}

/// Compiled once, here, as [`parse_value`] is.
pub fn value_hash(value: &serde_json::Value) -> Result<String, String> {
    canonical_hash(value)
}

fn ahead<T>(value: serde_json::Value, how: crate::Ahead) -> Result<(T, Option<ReadAhead>), crate::Refusal>
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
    static NOTED_READ: std::cell::Cell<Option<ReadAhead>> = const { std::cell::Cell::new(None) };
}

/// The kernel's load uses this. A copy read ahead is held under its stored
/// identity, and one read through a read-only step refuses a write (the
/// kernel's `Kind::require_writable`).
pub fn noting_read<R>(read: impl FnOnce() -> R) -> (R, Option<ReadAhead>) {
    NOTED_READ.with(|noted| noted.set(None));
    let answer = read();
    (answer, NOTED_READ.with(|noted| noted.take()))
}

pub fn versioned<T>(value: serde_json::Value) -> Result<T, crate::Refusal>
where
    T: serde::de::DeserializeOwned + crate::Versioned,
{
    read_value::<T>(value).map(|(document, _)| document)
}

pub fn read_value<T>(value: serde_json::Value) -> Result<(T, Option<ReadAhead>), crate::Refusal>
where
    T: serde::de::DeserializeOwned + crate::Versioned,
{
    match crate::reads::<T>(version_in::<T>(&value))? {
        crate::ReadAs::Current => {
            serde_json::from_value(value).map(|document| (document, None)).map_err(|e| format!("invalid document: {e}").into())
        }
        crate::ReadAs::Ahead(how) => ahead(value, how),
    }
}


pub fn version_in<T: crate::Versioned>(value: &serde_json::Value) -> u32 {
    T::version_in(value)
}


