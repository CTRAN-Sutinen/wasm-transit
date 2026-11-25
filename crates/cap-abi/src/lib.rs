//! cap-abi: the kernel and the ABI of a capability module.
//!
//! Every module has four exports and no imports, so a host needs little
//! code to call it. This crate supplies the exports as a macro, so a
//! capability crate does not copy them.
//!
//! * `transit_alloc(len) -> ptr` and `transit_free(ptr, len)` manage the
//!   module's memory.
//! * `transit_invoke(ptr, len) -> ptr` reads a JSON request. It writes
//!   the answer as `[u32 LE len][{"ok": ...} | {"error": ...}]`.
//! * `transit_invoke_bytes(op, ptr, len) -> ptr` reads a bulk payload as
//!   raw bytes, for example a feed zip, an rt payload, or a gzipped
//!   network.
//!
//! A module is an assembly of one or more capabilities over the kernel
//! ([`kernel`]). The kernel owns the registries (one per document kind)
//! and the loads that fill them, and routes each call. A capability's
//! own bulk ops number 0–255 (an assembly of several carries the
//! capability's index in the high 16 bits). Kernel ops number from 256
//! and mean the same in every module: the loads for the kinds that its
//! capabilities read (`transit_version::Load::CODE`), and the registry
//! ops [`registry::OP_EVICT`] (257) and [`registry::OP_HELD`] (258).
//!
//! A capability crate registers its capability with `capability!`:
//!
//! ```ignore
//! // In the capability crate's lib.rs:
//! cap_abi::capability! {
//!     id: "board",
//!     json: dispatch_json,
//!     bytes: dispatch_bytes,
//!     documents: [Network, EditDocument, LiveDay, Watchlist],
//!     request: Request,
//! }
//! ```
//!
//! `gen-assemblies` generates the assembly crates. Each one declares the
//! four exports once, for all the capabilities that it links:
//!
//! ```ignore
//! cap_abi::assembly!(cap_board::CAPABILITY, cap_rt::CAPABILITY);
//! ```
//!
//! The unsafe exports live in this crate. A capability crate forbids
//! `unsafe`.

/// The generated `invoke`, `invoke_bytes`, and `registries` answer as the
/// one-capability module answers, including the kernel's loads. The crate's
/// tests and native hosts use them.
///
/// `bytes` takes the capability's own op code (0–255). `documents` names the
/// types of the kinds that the capability reads, and a stateless capability
/// gives `[]`. `consumes`, `speaks`, and `migrates` are each `&[]` when left
/// out (see [`kernel::Declared`]). The macro reads `OPS.md` from the root of
/// the crate.
#[macro_export]
macro_rules! capability {
    (
        id: $id:literal,
        json: $json:path,
        bytes: $bytes:path,
        documents: [$($kind:ty),* $(,)?],
        request: $request:ty
        $(, consumes: $consumes:expr)?
        $(, speaks: $speaks:expr)?
        $(, migrates: $migrates:expr)?
        $(,)?
    ) => {
        pub static CAPABILITY: $crate::kernel::Cap = $crate::kernel::Cap {
            id: $id,
            json: $json,
            bytes: $bytes,
            documents: &[$($crate::kernel::loader::<$kind>),*],
        };

        /// The generator reads this to write the manifest.
        pub fn declaration() -> $crate::kernel::Declared {
            $crate::kernel::Declared {
                id: $id,
                invoke,
                invoke_bytes,
                ops: $crate::kernel::op_names::<$request>(),
                documents: CAPABILITY.documents.iter().map(|k| k().id()).collect(),
                consumes: $crate::__or!([$($consumes)?] &[]),
                speaks: $crate::__or!([$($speaks)?] &[]),
                migrates: $crate::__or!([$($migrates)?] &[]),
                docs: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/OPS.md")),
            }
        }

        /// Answer a JSON request as this capability's one-capability
        /// module does.
        pub fn invoke(text: &str) -> String {
            $crate::kernel::invoke(&[&CAPABILITY], text)
        }

        /// Answer a bulk request as this capability's one-capability
        /// module does.
        pub fn invoke_bytes(op: u32, bytes: &[u8]) -> String {
            $crate::kernel::invoke_bytes(&[&CAPABILITY], op, bytes)
        }

        /// The registries of this capability's one-capability module.
        pub fn registries() -> Vec<&'static dyn $crate::registry::Held> {
            $crate::kernel::registries(&[&CAPABILITY])
        }
    };
}

/// The given expression, or the default when none is given.
#[doc(hidden)]
#[macro_export]
macro_rules! __or {
    ([] $default:expr) => {
        $default
    };
    ([$given:expr] $default:expr) => {
        $given
    };
}

/// Emit the four exports over the capabilities that an assembly links, in
/// index order. The bulk ops of a capability carry its index in the high 16
/// bits. A generated assembly crate uses this macro once.
#[macro_export]
macro_rules! assembly {
    ($($cap:path),+ $(,)?) => {
        /// The capabilities that this module links. Their order is their index.
        pub static CAPS: &[&$crate::kernel::Cap] = &[$(&$cap),+];

        /// A result buffer starts with a u32 length prefix.
        const LEN_PREFIX: usize = 4;

        /// Many allocators return null for a zero-size request. Rounding
        /// the size up to one byte makes alloc and free match.
        fn layout(len: usize) -> std::alloc::Layout {
            std::alloc::Layout::from_size_align(len.max(1), 1)
                .expect("allocation is always valid")
        }

        #[no_mangle]
        pub extern "C" fn transit_alloc(len: usize) -> *mut u8 {
            let ptr = unsafe { std::alloc::alloc(layout(len)) };
            if ptr.is_null() {
                std::alloc::handle_alloc_error(layout(len));
            }
            ptr
        }

        /// # Safety
        /// `ptr` must come from [`transit_alloc`] or a result buffer,
        /// with the length it was allocated with.
        #[no_mangle]
        pub unsafe extern "C" fn transit_free(ptr: *mut u8, len: usize) {
            if !ptr.is_null() {
                std::alloc::dealloc(ptr, layout(len));
            }
        }

        fn write_result(json: String) -> *mut u8 {
            let payload = json.as_bytes();
            let total = LEN_PREFIX + payload.len();
            let ptr = transit_alloc(total);
            unsafe {
                let buf = std::slice::from_raw_parts_mut(ptr, total);
                buf[..LEN_PREFIX].copy_from_slice(&(payload.len() as u32).to_le_bytes());
                buf[LEN_PREFIX..].copy_from_slice(payload);
            }
            ptr
        }

        /// # Safety
        /// `ptr` must point to `len` readable UTF-8 bytes.
        #[no_mangle]
        pub unsafe extern "C" fn transit_invoke(ptr: *const u8, len: usize) -> *mut u8 {
            write_result(match std::str::from_utf8(std::slice::from_raw_parts(ptr, len)) {
                Ok(text) => $crate::kernel::invoke(CAPS, text),
                Err(_) => r#"{"error":{"message":"invalid request: payload is not UTF-8"}}"#.to_string(),
            })
        }

        /// # Safety
        /// `ptr` must point to `len` readable bytes.
        #[no_mangle]
        pub unsafe extern "C" fn transit_invoke_bytes(op: u32, ptr: *const u8, len: usize) -> *mut u8 {
            write_result($crate::kernel::invoke_bytes(CAPS, op, std::slice::from_raw_parts(ptr, len)))
        }
    };
}


// The library of each kind reads and hashes its own kind.
pub use transit_version::document::{
    canonical_bytes, canonical_hash, content_hash, is_gzip, noting_read, parse_document, parse_versioned, read_versioned,
    versioned, ReadAhead,
};
use transit_version::document::read_value;
pub use transit_version::jcs;
pub use transit_version::{Refusal, RefusalCode};

/// A record that does not serialize gets a name that never equals a hash
/// (D-190).
pub fn record_hash(value: &serde_json::Value) -> String {
    canonical_hash(value).unwrap_or_else(|e| format!("unhashable: {e}"))
}

/// A host that serves the document under its hash serves these bytes. To
/// hash it, hash the document inside (`.0`). It writes its bytes only into
/// an answer that [`outcome`] writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Canonical<T>(pub T);

impl<T: serde::Serialize> serde::Serialize for Canonical<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error;
        let text = jcs::to_string(&self.0).map_err(S::Error::custom)?;
        let raw = serde_json::value::RawValue::from_string(text).map_err(S::Error::custom)?;
        raw.serialize(s)
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Canonical<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(Canonical)
    }
}

impl<T> std::ops::Deref for Canonical<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// Any version other than the one this build reads, or the next one that it
/// reads ahead ([`versioned`]), fails the request (docs/FORMATS.md,
/// "Versions"). An op that writes over a document uses [`Base`]. On the wire
/// it is the document itself (`#[ts(as = "T")]` on the field).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct Current<T>(pub T);

impl<'de, T> serde::Deserialize<'de> for Current<T>
where
    T: serde::de::DeserializeOwned + transit_version::Versioned,
{
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        versioned(<serde_json::Value as serde::Deserialize>::deserialize(d)?).map(Current).map_err(stash)
    }
}

impl<T> std::ops::Deref for Current<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// Reads like [`Current`], and also refuses a copy read ahead through a
/// read-only step.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct Base<T>(pub T);

impl<'de, T> serde::Deserialize<'de> for Base<T>
where
    T: serde::de::DeserializeOwned + transit_version::Versioned,
{
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (document, read) = read_value::<T>(<serde_json::Value as serde::Deserialize>::deserialize(d)?).map_err(stash)?;
        if let Some(read) = read {
            transit_version::writes_over::<T>(read.found, transit_version::ReadAs::Ahead(read.how)).map_err(stash)?;
        }
        Ok(Base(document))
    }
}

/// Also return the fields that this build does not know.
pub fn read_base<T: serde::de::DeserializeOwned + transit_version::Versioned>(doc: serde_json::Value) -> Result<(T, Vec<String>), Refusal> {
    let (read, unknown) = transit_version::noting_unknown(|| serde_json::from_value::<Base<T>>(doc));
    Ok((read.map_err(|e| stashed().unwrap_or_else(|| e.to_string().into()))?.0, unknown))
}

thread_local! {
    static STASHED: std::cell::RefCell<Option<Refusal>> = const { std::cell::RefCell::new(None) };
}

/// serde carries only text, so a deserializer that refuses a document keeps
/// the refusal here, and the caller answers it with its code.
fn stash<E: serde::de::Error>(refusal: Refusal) -> E {
    let error = E::custom(&refusal.message);
    STASHED.with(|stashed| *stashed.borrow_mut() = Some(refusal));
    error
}

fn stashed() -> Option<Refusal> {
    STASHED.with(|stashed| stashed.borrow_mut().take())
}

impl<T> std::ops::Deref for Base<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// For an older document, the error also names the op that migrates it. A
/// newer document came from a newer build, and nothing rewrites it.
pub fn speaks(kind: &str, found: u32, reads: u32) -> Result<(), transit_version::Refusal> {
    if found == reads {
        Ok(())
    } else {
        Err(transit_version::refused(kind, found, reads))
    }
}

/// A request that does not decode gives an error and does not panic.
pub fn answer<Q: serde::de::DeserializeOwned, A: serde::Serialize, E: Into<Refusal>>(
    text: &str,
    dispatch: impl FnOnce(Q) -> Result<A, E>,
) -> String {
    stashed();
    let (request, unknown) = transit_version::noting_unknown(|| serde_json::from_str::<Q>(text));
    REQUEST_UNKNOWN.with(|noted| *noted.borrow_mut() = unknown);
    let answered = match request {
        Ok(request) => outcome(dispatch(request)),
        Err(error) => match stashed() {
            Some(refusal) => outcome::<(), Refusal>(Err(refusal.context("invalid request"))),
            None => outcome::<(), String>(Err(format!("invalid request: {error}"))),
        },
    };
    REQUEST_UNKNOWN.with(|noted| noted.borrow_mut().clear());
    answered
}

thread_local! {
    static REQUEST_UNKNOWN: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A `validate` answers these fields, so the write path flags them for a
/// person. The list is empty outside [`answer`].
pub fn request_unknown_fields() -> Vec<String> {
    REQUEST_UNKNOWN.with(|noted| noted.borrow().clone())
}

/// It serializes directly, because an intermediate `serde_json::Value` would
/// copy the answer and change a document that the answer carries as its
/// canonical bytes ([`Canonical`], docs/FORMATS.md).
pub fn outcome<T: serde::Serialize, E: Into<Refusal>>(result: Result<T, E>) -> String {
    #[derive(serde::Serialize)]
    enum Outcome<T> {
        #[serde(rename = "ok")]
        Ok(T),
        #[serde(rename = "error")]
        Error(Refusal),
    }
    let outcome = match result {
        Ok(value) => Outcome::Ok(value),
        Err(error) => Outcome::Error(error.into()),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

pub mod kernel;
pub mod kinds;
pub mod manifest;
pub mod registry;
pub mod validate;
