//! transit-wasm — the wasm artifact.
//!
//! One module, zero imports, four exports: a flat C ABI over
//! transit-ops' [`dispatch`](`transit_ops::dispatch`) seam. The bytes
//! produced by `cargo build --release --target wasm32-unknown-unknown`
//! are THE deployment artifact: the browser fetches this exact file,
//! the Go sync server loads this exact file into wazero, and the
//! manifest pins its sha256. Nothing host-specific ever touches the
//! module — no wasm-bindgen glue, so no glue-version drift and no
//! imports a non-JS runtime would have to stub.
//!
//! Hosts own I/O and the clock (see transit-ops' docs); this module
//! owns domain logic and nothing else.
//!
//! Protocol:
//!
//! * `transit_alloc(len) -> ptr` — allocate a staging buffer; host
//!   writes its payload there (JS: `new Uint8Array(memory, ptr, len)`,
//!   Go: `mem.Write`).
//! * `transit_free(ptr, len)` — release a staging buffer or a result.
//! * `transit_invoke(ptr, len) -> ptr` — a serialized
//!   [`Request`](`transit_ops::Request`) (JSON) in; a result buffer
//!   out.
//! * `transit_invoke_bytes(op, ptr, len) -> ptr` — the bulk-payload
//!   form (feed zips, street GeoJSON), where JSON arrays of numbers
//!   would be absurd. Op codes: 0 = load_feed. (The rt protobuf
//!   payload moved to the cap-rt capability with its decoder.)
//!
//! Result buffers are `[u32 little-endian length][payload]`; the host
//! reads the length, copies the payload, then frees the buffer with
//! `transit_free(ptr, 4 + len)`. Payload shape (see
//! `transit_ops::dispatch_json`): `{"ok": <Response>}` on success,
//! `{"error": "<message>"}` on failure — hosts have exactly two
//! paths.
//!
//! Panics inside wasm trap the call (host runtimes surface them as
//! errors); the dispatch layer itself reports failures as
//! `{"error": ...}` instead of panicking.

use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::slice;

/// Result buffers carry a u32 length prefix.
const LEN_PREFIX: usize = 4;

/// Many allocators return null for zero-size requests; normalize so
/// alloc/free stay symmetric on both sides.
fn layout(len: usize) -> Layout {
    Layout::from_size_align(len.max(1), 1).expect("allocation is always valid")
}

#[no_mangle]
pub extern "C" fn transit_alloc(len: usize) -> *mut u8 {
    let ptr = unsafe { alloc(layout(len)) };
    if ptr.is_null() {
        handle_alloc_error(layout(len));
    }
    ptr
}

/// # Safety
/// `ptr` must come from [`transit_alloc`] or a result buffer, with the
/// length it was allocated with.
#[no_mangle]
pub unsafe extern "C" fn transit_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        dealloc(ptr, layout(len));
    }
}

/// Serialize an outcome into a fresh `[len][payload]` buffer.
fn write_result(json: String) -> *mut u8 {
    let payload = json.as_bytes();
    let total = LEN_PREFIX + payload.len();
    let ptr = transit_alloc(total);
    unsafe {
        let buf = slice::from_raw_parts_mut(ptr, total);
        buf[..LEN_PREFIX].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        buf[LEN_PREFIX..].copy_from_slice(payload);
    }
    ptr
}

/// # Safety
/// `ptr` must point to `len` readable bytes (UTF-8 for the JSON ops).
#[no_mangle]
pub unsafe extern "C" fn transit_invoke(ptr: *const u8, len: usize) -> *mut u8 {
    let input = match std::str::from_utf8(slice::from_raw_parts(ptr, len)) {
        Ok(text) => transit_ops::dispatch_json(text),
        Err(_) => r#"{"error":"invalid request: payload is not UTF-8"}"#.to_string(),
    };
    write_result(input)
}

/// # Safety
/// `ptr` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn transit_invoke_bytes(op: u32, ptr: *const u8, len: usize) -> *mut u8 {
    let bytes = slice::from_raw_parts(ptr, len);
    write_result(transit_ops::dispatch_bytes(op, bytes))
}
