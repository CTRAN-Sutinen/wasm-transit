//! cap-abi — the capability-module ABI shell.
//!
//! The capability-module ABI: four exports, zero imports, every
//! host trivial. A capability differs in *which ops* its dispatch
//! answers, never in how it is called — so the shell is this macro
//! instead of a copy per crate.
//!
//! Protocol (the same contract every module speaks):
//!
//! * `transit_alloc(len) -> ptr`, `transit_free(ptr, len)`;
//! * `transit_invoke(ptr, len) -> ptr` — JSON request in,
//!   `[u32 LE len][{"ok": ...} | {"error": ...}]` out;
//! * `transit_invoke_bytes(op, ptr, len) -> ptr` — the bulk-payload
//!   form for feed bytes (a JSON array of numbers would be absurd)
//!   and for the kernel's own documents ([`OP_LOAD_NETWORK`]).
//!
//! A capability's own bulk ops number from 0; kernel ops number from
//! 256 and mean the same thing in every module that answers them.
//!
//! Usage:
//!
//! ```ignore
//! mod my_cap {
//!     pub fn dispatch_json(text: &str) -> String { ... }
//!     pub const OP_LOAD_FEED: u32 = 0;
//!     pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String { ... }
//! }
//!
//! // In the capability crate's lib.rs:
//! cap_abi::capability! {
//!     json: my_cap::dispatch_json,
//!     bytes: (my_cap::OP_LOAD_FEED, my_cap::dispatch_bytes),
//! }
//! ```
//!
//! One byte payload for op codes keeps hosts dumb; the manifest
//! advertises which ops each capability answers.

/// Emit the four-export flat ABI over a capability's dispatch pair.
///
/// `json` is `fn(&str) -> String`; `bytes` is `fn(u32, &[u8]) ->
/// String` (the op code is the module's own public const, advertised
/// in the manifest; hosts pass it through to `transit_invoke_bytes`
/// uninterpreted).
#[macro_export]
macro_rules! capability {
    (json: $json:path, bytes: $bytes:path $(,)?) => {
        /// Result buffers carry a u32 length prefix.
        const LEN_PREFIX: usize = 4;

        /// Many allocators return null for zero-size requests;
        /// normalize so alloc/free stay symmetric on both sides.
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

        /// Serialize an outcome into a fresh `[len][payload]` buffer.
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
            let input = match std::str::from_utf8(std::slice::from_raw_parts(ptr, len)) {
                Ok(text) => $json(text),
                Err(_) => r#"{"error":"invalid request: payload is not UTF-8"}"#.to_string(),
            };
            write_result(input)
        }

        /// # Safety
        /// `ptr` must point to `len` readable bytes.
        #[no_mangle]
        pub unsafe extern "C" fn transit_invoke_bytes(op: u32, ptr: *const u8, len: usize) -> *mut u8 {
            let bytes = std::slice::from_raw_parts(ptr, len);
            write_result($bytes(op, bytes))
        }
    };
}

/// Kernel bulk op: register the network document from its JSON bytes.
/// Every capability that answers `load_network` answers this too,
/// with the same response and the same hash.
///
/// The bytes are the bare document (no request envelope), parsed
/// straight into the typed model. The JSON op's `{"op": ..,
/// "network": ..}` envelope is an internally tagged enum, which serde
/// can only decode by buffering the whole input into a generic tree
/// first — on a real network that peak is ~8× the document, and wasm
/// memory never shrinks back from a peak.
pub const OP_LOAD_NETWORK: u32 = 256;

/// The JSON lane's `load_network`, peeled off before the typed decode:
/// the envelope is read with the document left as a borrowed raw
/// slice and handed to `load` — the same direct parse as the bulk op,
/// so registration costs the same on either lane and a module carries
/// one `Network` decoder, not two. `None` for every other op (which
/// then decodes as usual); the module's `LoadNetwork` request variant
/// is `skip_deserializing` so the buffered decode is never compiled.
pub fn load_network_json(text: &str, load: impl FnOnce(&[u8]) -> String) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Envelope<'a> {
        #[serde(borrow)]
        op: std::borrow::Cow<'a, str>,
        #[serde(borrow)]
        network: Option<&'a serde_json::value::RawValue>,
    }
    let envelope: Envelope = serde_json::from_str(text).ok()?;
    if envelope.op != "load_network" {
        return None;
    }
    Some(match envelope.network {
        Some(network) => load(network.get().as_bytes()),
        None => outcome::<()>(Err("invalid request: missing field `network`".to_string())),
    })
}

/// "sha256:<hex>" of bytes — the content-hash formula every module
/// and host shares.
pub fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// A document's identity: the content hash of its canonical
/// serialization (serde_json, struct field order). Streamed into the
/// digest, so hashing never holds a second copy of the document —
/// byte-identical to hashing `serde_json::to_string(value)`.
pub fn canonical_hash<T: serde::Serialize>(value: &T) -> Result<String, String> {
    use sha2::{Digest, Sha256};

    struct Digesting(Sha256);
    impl std::io::Write for Digesting {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.update(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut digest = Digesting(Sha256::new());
    serde_json::to_writer(&mut digest, value).map_err(|e| format!("document does not serialize: {e}"))?;
    Ok(format!("sha256:{:x}", digest.0.finalize()))
}

/// Parse a kernel document from bulk bytes, straight into its type.
pub fn parse_document<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("invalid document: {e}"))
}

/// Parse a network document from bulk bytes: straight into the type,
/// and — only when that fails — through `read_network`, which reads
/// documents written before trips were stored (a host's saved
/// documents load after the model changed).
pub fn parse_network(bytes: &[u8]) -> Result<transit_network::Network, String> {
    parse_document(bytes).or_else(|typed: String| {
        let text = std::str::from_utf8(bytes).map_err(|e| format!("invalid document: {e}"))?;
        transit_network::read_network(text).map_err(|_| typed)
    })
}

/// The `{"ok": ..} | {"error": ..}` outcome every op answers,
/// serialized directly (no intermediate `serde_json::Value`).
pub fn outcome<T: serde::Serialize>(result: Result<T, String>) -> String {
    #[derive(serde::Serialize)]
    enum Outcome<T> {
        #[serde(rename = "ok")]
        Ok(T),
        #[serde(rename = "error")]
        Error(String),
    }
    let outcome = match result {
        Ok(value) => Outcome::Ok(value),
        Err(error) => Outcome::Error(error),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

pub mod manifest;
pub mod validate;
