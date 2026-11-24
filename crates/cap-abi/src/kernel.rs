//! The kernel: the registries every capability reads, the loads that
//! fill them, and the routing an assembly's four exports run.
//!
//! **One registry per document kind, owned here.** A capability names
//! the kinds it reads by their types
//! (`capability! { documents: [Network, EditDocument] }`) and reads them
//! through [`kind`] ([`Kind::require`]). It cannot declare a registry: the
//! constructor is private to this crate. In one assembly, every
//! capability reads the same `Arc`, so a document registers once per
//! instance, however many capabilities read it (D-77).
//!
//! **The kernel knows no kind (D-239).** Each kind's library implements
//! `transit_version::Load`: the kind's id, its load op and code, how it
//! reads the bytes, and what the load answers. The kernel keeps one
//! registry for each type and depends on no domain library.
//!
//! **Registration is the kernel's.** Each kind's load is a kernel op: a
//! bulk op from 256, and the same JSON op
//! (`{"op": "load_network", "network": …}`) on the JSON lane. The kernel
//! peels it before any capability decodes the request, so a capability's
//! `Request` carries no load op. A load answers the document's identity
//! (`<kind>_hash`, the module is the hash authority) and what the host
//! shows about it (a feed's summary, a layer's fields).
//!
//! **Derived data is held here too.** A derivation is a library's pure
//! function and its key: the day patched with the edits in force, the
//! router's graph, a street graph. [`derive`] finds the entry or
//! computes it, bounded per [`Derivation`], and drops it when a keep-set
//! drops any document it came from. The answer is the same either way:
//! an entry is a cache, never a dependency
//! (assembly independence, `crates/cap-abi/independence.mjs`).
//!
//! **Routing.** An assembly is one or more [`Cap`]s. A JSON request
//! names its capability (`{"cap": "board", "op": …}`) unless the
//! assembly holds one. A capability's bulk op carries its index in the
//! high 16 bits (its own codes are 0–255). The kernel ops (256 and up)
//! name none. [`invoke`] and [`invoke_bytes`] are what the exports run
//! (`assembly!`), and what native tests call.

use std::any::Any;
use std::borrow::Cow;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use ts_rs::TS;

pub use transit_version::{Form, Load};

use crate::registry::{Held, Registry, Retain, OP_EVICT, OP_HELD};

// --- the kernel ops ---

/// Kernel bulk op: the capabilities the module links, in index order, each
/// with the kinds it reads (the bytes are ignored). The answer is
/// [`About`]. The kinds' loads take their codes from their libraries
/// (`transit_version::Load::CODE`), 256 and up.
pub const OP_ABOUT: u32 = 268;

/// [`OP_ABOUT`]'s answer.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct About {
    pub capabilities: Vec<AboutCap>,
}

/// One linked capability: its id and the kinds it reads.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct AboutCap {
    pub id: String,
    pub documents: Vec<String>,
}

/// A capability's own bulk ops number below this; the kernel's from it.
pub const KERNEL_OPS: u32 = 256;

/// A load's whole answer: the op, what the kind answers
/// (`transit_version::Load::answer`: its hash under the kind's field, and
/// what the host shows about it), and the fields the document carries that
/// this build does not know (D-167). The document keeps them; a receive
/// path flags them for a person. Absent when every field is known.
#[derive(Serialize)]
struct LoadAnswer<A> {
    op: &'static str,
    #[serde(flatten)]
    loaded: A,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unknown_fields: Vec<String>,
}

/// The JSON lane's envelope: the op and the capability it names. Every
/// other field is skipped, so a request that is not a load costs one
/// scan. A load reads its document's field again ([`Loader::load_json`]).
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow)]
    op: Cow<'a, str>,
    #[serde(borrow, default)]
    cap: Option<Cow<'a, str>>,
}

/// The field a load needs, as raw JSON, or the request error naming it.
fn field<'a>(text: &'a str, name: &str) -> Result<&'a RawValue, String> {
    let fields: std::collections::HashMap<Cow<'a, str>, &'a RawValue> =
        serde_json::from_str(text).map_err(|e| format!("invalid request: {e}"))?;
    fields.get(name).copied().ok_or_else(|| format!("invalid request: missing field `{name}`"))
}

// --- the kinds ---

/// One document kind's registry. Its library says what the kind is and
/// how it loads (`transit_version::Load`, D-239).
pub struct Kind<T: 'static> {
    registry: Registry<T>,
}

/// The registry of kind `T`: one for each type in an instance, made the
/// first time a capability or the routing asks.
pub fn kind<T: Load>() -> &'static Kind<T> {
    use std::any::{Any, TypeId};
    use std::sync::Mutex;
    static KINDS: Mutex<Vec<(TypeId, &'static (dyn Any + Send + Sync))>> = Mutex::new(Vec::new());
    let mut kinds = KINDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let id = TypeId::of::<T>();
    if let Some((_, held)) = kinds.iter().find(|(t, _)| *t == id) {
        return held.downcast_ref::<Kind<T>>().expect("a registry is held under its own type");
    }
    let retain = match T::RETAIN {
        transit_version::Retain::Host => Retain::Host,
        transit_version::Retain::Last(n) => Retain::Last(n),
    };
    let made: &'static Kind<T> = Box::leak(Box::new(Kind { registry: Registry::documents(T::NAME, T::LOAD_OP, retain) }));
    kinds.push((id, made));
    made
}

/// The registry of kind `T`, as the routing sees it: what
/// `capability! { documents: [...] }` lists.
pub fn loader<T: Load>() -> &'static dyn Loader {
    kind::<T>()
}

impl<T: Load> Kind<T> {
    /// The document under `hash`, or the shared unknown-hash error
    /// (`unknown <kind> <hash>: <load op> first`).
    pub fn require(&self, hash: &str) -> Result<Arc<T>, String> {
        self.registry.require(hash)
    }

    pub fn get(&self, hash: &str) -> Option<Arc<T>> {
        self.registry.get(hash)
    }

    /// The document under `hash` as the base of a write. A copy read one
    /// version ahead through a read-only step is refused, naming both
    /// versions (D-186). Otherwise it is [`Self::require`].
    pub fn require_writable(&self, hash: &str) -> Result<Arc<T>, String>
    where
        T: transit_version::Versioned,
    {
        let held = self.require(hash)?;
        match self.registry.ahead(hash) {
            Some((found, transit_version::Ahead::ReadOnly)) => Err(transit_version::write_refused(T::KIND, found, T::READS)),
            _ => Ok(held),
        }
    }

    /// The version the document under `hash` was in, where it was read
    /// one version ahead (D-186), and how. It is held under its stored
    /// identity, stepped down.
    pub fn read_ahead(&self, hash: &str) -> Option<(u32, transit_version::Ahead)> {
        self.registry.ahead(hash)
    }

    /// Register a document an op minted (an import's network, cap-rt's
    /// live document) and answer its identity, so the page holds a handle
    /// and the document need not cross back in.
    pub fn register(&self, document: T) -> Result<(String, Arc<T>), String>
    where
        T: Serialize,
    {
        let hash = crate::canonical_hash(&document)?;
        let held = self.registry.insert(hash.clone(), document);
        Ok((hash, held))
    }

    /// Register a byte document under the content hash of its bytes
    /// (idempotent: bytes held already are not parsed again).
    pub fn register_bytes(&self, bytes: &[u8]) -> Result<(String, Arc<T>), String> {
        let hash = crate::content_hash(bytes);
        if let Some(held) = self.registry.get(&hash) {
            return Ok((hash, held));
        }
        let held = self.registry.insert(hash.clone(), T::parse(bytes)?);
        Ok((hash, held))
    }

    /// How many documents of the kind are held.
    pub fn len(&self) -> usize {
        self.registry.len()
    }

    pub fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }

    /// The load: the identity, the held document, and the unknown fields.
    fn load(&self, bytes: &[u8]) -> Result<(String, Arc<T>, Vec<String>), String> {
        match T::FORM {
            Form::Json { .. } => {
                let ((document, unknown_fields), ahead) =
                    crate::noting_read(|| transit_version::noting_unknown(|| T::parse(bytes)));
                let document = document?;
                // A copy read one version ahead keeps the identity it is
                // stored under (D-186). A read-only one refuses a write.
                let hash = match &ahead {
                    Some(read) => read.hash.clone(),
                    None => document.canonical().unwrap_or_else(|| Err(format!("{}: a JSON kind answers its canonical hash", T::ID)))?,
                };
                if let Some(read) = ahead {
                    self.registry.mark_ahead(&hash, read.found, read.how);
                }
                Ok((hash.clone(), self.registry.insert(hash, document), unknown_fields))
            }
            Form::Bytes => {
                let (hash, held) = self.register_bytes(bytes)?;
                Ok((hash, held, vec![]))
            }
        }
    }

    fn answer(&self, loaded: Result<(String, Arc<T>, Vec<String>), String>) -> String {
        crate::outcome(loaded.map(|(hash, held, unknown_fields)| LoadAnswer { op: T::LOAD_OP, loaded: held.answer(hash), unknown_fields }))
    }
}

/// A kind seen by the routing, whatever it holds.
pub trait Loader: Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn load_op(&self) -> &'static str;
    fn op(&self) -> u32;
    fn held(&self) -> &dyn Held;
    fn load_bytes(&self, bytes: &[u8]) -> String;
    /// The load on the JSON lane: `text` is the whole request.
    fn load_json(&self, text: &str) -> String;
}

impl<T: Load> Loader for Kind<T> {
    fn id(&self) -> &'static str {
        T::ID
    }
    fn name(&self) -> &'static str {
        T::NAME
    }
    fn load_op(&self) -> &'static str {
        T::LOAD_OP
    }
    fn op(&self) -> u32 {
        T::CODE
    }
    fn held(&self) -> &dyn Held {
        &self.registry
    }
    fn load_bytes(&self, bytes: &[u8]) -> String {
        self.answer(self.load(bytes))
    }
    fn load_json(&self, text: &str) -> String {
        let loaded = match T::FORM {
            Form::Json { field: name } => field(text, name).and_then(|raw| self.load(raw.get().as_bytes())),
            // A byte document on the JSON lane: an array of numbers.
            Form::Bytes => field(text, "bytes")
                .and_then(|raw| serde_json::from_str::<Vec<u8>>(raw.get()).map_err(|e| format!("invalid request: bytes: {e}")))
                .and_then(|bytes| self.load(&bytes)),
        };
        self.answer(loaded)
    }
}

// --- derived entries ---

/// Every derived entry, whatever it holds, by `<derivation>|<key>`.
static DERIVED: Registry<dyn Any + Send + Sync> = Registry::derived("derived", Retain::Host);

pub fn derived_registry() -> &'static dyn Held {
    &DERIVED
}

/// One derivation: its name (the entry's key prefix) and how many entries
/// an instance keeps on its own (D-70).
pub struct Derivation {
    pub name: &'static str,
    pub keep: usize,
}

/// The day: the network materialized for a date, patched with the
/// edits in force on it (`transit_dispatch::day`).
pub const DAY: Derivation = Derivation { name: "day", keep: 4 };
/// The router's RAPTOR graph over a day, its predictions, and its walks.
pub const GRAPH: Derivation = Derivation { name: "graph", keep: 8 };
/// The street graph (snap, route) over a streets document.
pub const STREET_GRAPH: Derivation = Derivation { name: "street graph", keep: 4 };
/// The walking graph over a streets document.
pub const WALK_GRAPH: Derivation = Derivation { name: "walk graph", keep: 4 };

/// The entry `derivation` holds under `key`, or `compute`'s answer,
/// held from now on. `inputs` are the document hashes it was computed
/// from: a keep-set that drops any of them drops the entry. The value is
/// the same whoever computes it: a pure function of the documents the key
/// names.
pub fn derive<T: Any + Send + Sync>(
    derivation: &Derivation,
    key: &str,
    inputs: &[&str],
    compute: impl FnOnce() -> Result<T, String>,
) -> Result<Arc<T>, String> {
    let full = format!("{}|{key}", derivation.name);
    if let Some(held) = DERIVED.get(&full) {
        if let Ok(value) = held.downcast::<T>() {
            return Ok(value);
        }
    }
    let value = Arc::new(compute()?);
    DERIVED.insert_derived(full, inputs.iter().map(|s| s.to_string()).collect(), value.clone() as Arc<dyn Any + Send + Sync>);
    let prefix = format!("{}|", derivation.name);
    DERIVED.bound(derivation.keep, |key| key.starts_with(&prefix));
    Ok(value)
}

/// How many entries the derivation holds.
pub fn derived_len(derivation: &Derivation) -> usize {
    let prefix = format!("{}|", derivation.name);
    DERIVED.keys().iter().filter(|k| k.starts_with(&prefix)).count()
}

/// Drop the derivation's entries `keep` refuses: a capability's own
/// rule over its entries (the router keeps only the newest live
/// document's graphs).
pub fn retain_derived(derivation: &Derivation, keep: impl Fn(&str, &[String]) -> bool) {
    let prefix = format!("{}|", derivation.name);
    DERIVED.retain(|key, inputs| match key.strip_prefix(&prefix) {
        Some(rest) => keep(rest, inputs),
        None => true,
    });
}

// --- capabilities and assemblies ---

/// One capability as an assembly links it: its id, its dispatch pair,
/// and the document kinds it reads, each as its registry ([`loader`]).
/// `capability!` declares it; `assembly!` routes to it.
pub struct Cap {
    pub id: &'static str,
    pub json: fn(&str) -> String,
    pub bytes: fn(u32, &[u8]) -> String,
    pub documents: &'static [fn() -> &'static dyn Loader],
}

/// A capability's whole declaration, as the generator reads it
/// (`capability!` writes it): its id, lanes, ops (the `Request` enum's
/// tags), kinds, and `OPS.md`. It is a function, not a static: no module
/// calls it, so no module carries the docs' text.
#[derive(Debug, Clone)]
pub struct Declared {
    pub id: &'static str,
    pub invoke: fn(&str) -> String,
    pub invoke_bytes: fn(u32, &[u8]) -> String,
    pub ops: Vec<String>,
    /// The kernel kinds it reads (`documents: [...]`).
    pub documents: Vec<&'static str>,
    /// Each kind it reads as another capability's answer, and that
    /// capability's id.
    pub consumes: &'static [(&'static str, &'static str)],
    /// Each kind it reads or writes, and the versions it reads.
    pub speaks: &'static [(&'static str, &'static [u32])],
    /// The version steps it carries for a kind (cap-migrate's alone).
    pub migrates: &'static [(&'static str, &'static [(u32, u32)])],
    /// Its `OPS.md`: the purpose, then one `##` section for each op.
    pub docs: &'static str,
}

/// The op tags of a capability's `Request` enum, in its order. Every
/// `Request` is tagged by `op` (`#[serde(tag = "op")]`), so serde's
/// refusal of an unknown tag names every tag it knows. This needs no
/// dependency, and the list cannot differ from what the dispatch decodes.
pub fn op_names<T: serde::de::DeserializeOwned>() -> Vec<String> {
    let refusal = match serde_json::from_str::<T>(r#"{"op":"\u0001"}"#) {
        Ok(_) => panic!("a Request decoded an op tag no capability has"),
        Err(e) => e.to_string(),
    };
    let expected = refusal
        .split_once("expected ")
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| panic!("serde's refusal names no expected ops: {refusal}"));
    let expected = expected.split(" at line ").next().unwrap_or(expected);
    expected.split('`').skip(1).step_by(2).map(str::to_string).collect()
}

/// The kinds an assembly's capabilities read, each once.
fn kinds(caps: &[&Cap]) -> Vec<&'static dyn Loader> {
    let mut out: Vec<&'static dyn Loader> = Vec::new();
    for cap in caps {
        for kind in cap.documents.iter().map(|k| k()) {
            if !out.iter().any(|k| k.id() == kind.id()) {
                out.push(kind);
            }
        }
    }
    out
}

/// The registries an assembly holds, in `held` order: each kind its
/// capabilities read, then the derived entries.
pub fn registries(caps: &[&Cap]) -> Vec<&'static dyn Held> {
    let mut out: Vec<&'static dyn Held> = kinds(caps).into_iter().map(|k| k.held()).collect();
    out.push(&DERIVED);
    out
}

fn error(message: String) -> String {
    crate::outcome::<()>(Err(message))
}

/// The JSON lane of an assembly: a kernel load when the op is one of its
/// kinds' loads, otherwise the capability the request names (or the only
/// one).
pub fn invoke(caps: &[&Cap], text: &str) -> String {
    let envelope: Option<Envelope> = serde_json::from_str(text).ok();
    if let Some(envelope) = &envelope {
        if let Some(kind) = kinds(caps).into_iter().find(|k| k.load_op() == envelope.op) {
            return kind.load_json(text);
        }
    }
    let named = envelope.as_ref().and_then(|e| e.cap.as_deref());
    match (named, caps) {
        (Some(id), _) => match caps.iter().find(|c| c.id == id) {
            Some(cap) => (cap.json)(text),
            None => error(format!(
                "invalid request: no capability {id:?} in this module (it links {})",
                caps.iter().map(|c| c.id).collect::<Vec<_>>().join(", ")
            )),
        },
        (None, [only]) => (only.json)(text),
        (None, _) => error(format!(
            "invalid request: name the capability (\"cap\": one of {})",
            caps.iter().map(|c| c.id).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// The bulk lane of an assembly: the registry ops, its kinds' loads, and
/// each capability's own bulk ops (index in the high 16 bits, code in the
/// low 8).
pub fn invoke_bytes(caps: &[&Cap], op: u32, bytes: &[u8]) -> String {
    if op == OP_EVICT || op == OP_HELD {
        return crate::registry::kernel(op, bytes, &registries(caps)).expect("evict and held are the registry ops");
    }
    if op == OP_ABOUT {
        let capabilities = caps
            .iter()
            .map(|c| AboutCap { id: c.id.to_string(), documents: c.documents.iter().map(|k| k().id().to_string()).collect() })
            .collect();
        return crate::outcome(Ok::<_, String>(About { capabilities }));
    }
    if let Some(kind) = kinds(caps).into_iter().find(|k| k.op() == op) {
        return kind.load_bytes(bytes);
    }
    let (index, code) = ((op >> 16) as usize, op & 0xffff);
    match caps.get(index) {
        Some(cap) if code < KERNEL_OPS => (cap.bytes)(code, bytes),
        _ => error(format!("unknown bulk op {op}")),
    }
}

/// The bulk op code a host sends for capability `index`'s own op `code`.
pub const fn cap_op(index: u32, code: u32) -> u32 {
    (index << 16) | code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo_a(text: &str) -> String {
        format!(r#"{{"ok":{{"cap":"a","text":{}}}}}"#, serde_json::to_string(text).unwrap())
    }
    fn echo_b(text: &str) -> String {
        format!(r#"{{"ok":{{"cap":"b","text":{}}}}}"#, serde_json::to_string(text).unwrap())
    }
    fn bytes_a(op: u32, _: &[u8]) -> String {
        format!(r#"{{"ok":{{"cap":"a","op":{op}}}}}"#)
    }
    fn bytes_b(op: u32, _: &[u8]) -> String {
        format!(r#"{{"ok":{{"cap":"b","op":{op}}}}}"#)
    }
    /// Two test kinds, since the kernel names no library's kind. A fence is a
    /// JSON kind under its own field; a watch is one under `document`.
    #[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Fence {
        #[serde(default)]
        posts: u32,
    }
    #[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Watch {
        #[serde(default)]
        stops: Vec<String>,
    }

    /// A plain JSON read: the test kinds carry no version.
    fn plain<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
        crate::parse_document(bytes)
    }

    #[derive(serde::Serialize)]
    struct FenceLoaded {
        fence_hash: String,
    }
    impl Load for Fence {
        const ID: &'static str = "fence";
        const NAME: &'static str = "fence";
        const LOAD_OP: &'static str = "load_fence";
        const CODE: u32 = 900;
        const FORM: Form = Form::Json { field: "fence" };
        type Answer = FenceLoaded;
        fn parse(bytes: &[u8]) -> Result<Self, String> {
            plain(bytes)
        }
        fn canonical(&self) -> Option<Result<String, String>> {
            Some(crate::canonical_hash(self))
        }
        fn answer(&self, fence_hash: String) -> FenceLoaded {
            FenceLoaded { fence_hash }
        }
    }

    #[derive(serde::Serialize)]
    struct WatchLoaded {
        watch_hash: String,
    }
    impl Load for Watch {
        const ID: &'static str = "watch";
        const NAME: &'static str = "watch";
        const LOAD_OP: &'static str = "load_watch";
        const CODE: u32 = 901;
        const FORM: Form = Form::Json { field: "document" };
        type Answer = WatchLoaded;
        fn parse(bytes: &[u8]) -> Result<Self, String> {
            plain(bytes)
        }
        fn canonical(&self) -> Option<Result<String, String>> {
            Some(crate::canonical_hash(self))
        }
        fn answer(&self, watch_hash: String) -> WatchLoaded {
            WatchLoaded { watch_hash }
        }
    }

    static A: Cap = Cap { id: "a", json: echo_a, bytes: bytes_a, documents: &[loader::<Fence>] };
    static B: Cap = Cap { id: "b", json: echo_b, bytes: bytes_b, documents: &[loader::<Fence>, loader::<Watch>] };

    fn ok(answer: String) -> serde_json::Value {
        let value: serde_json::Value = serde_json::from_str(&answer).unwrap();
        value.get("ok").cloned().unwrap_or_else(|| panic!("{value}"))
    }

    #[test]
    fn a_request_reaches_the_capability_it_names() {
        assert_eq!(ok(invoke(&[&A, &B], r#"{"cap":"b","op":"x"}"#))["cap"], "b");
        assert_eq!(ok(invoke(&[&A, &B], r#"{"cap":"a","op":"x"}"#))["cap"], "a");
        // One capability: the request need not name it.
        assert_eq!(ok(invoke(&[&B], r#"{"op":"x"}"#))["cap"], "b");
        let unnamed = invoke(&[&A, &B], r#"{"op":"x"}"#);
        assert!(unnamed.contains("name the capability"), "{unnamed}");
        let unknown = invoke(&[&A, &B], r#"{"cap":"c","op":"x"}"#);
        assert!(unknown.contains(r#"no capability \"c\""#), "{unknown}");
        // A request that is not an envelope reaches the capability,
        // whose own decode answers it.
        assert_eq!(ok(invoke(&[&A], "not json"))["text"], "not json");
    }

    #[test]
    fn a_bulk_op_carries_its_capability_index() {
        assert_eq!(ok(invoke_bytes(&[&A, &B], cap_op(1, 3), b"")), serde_json::json!({ "cap": "b", "op": 3 }));
        assert_eq!(ok(invoke_bytes(&[&A, &B], cap_op(0, 3), b"")), serde_json::json!({ "cap": "a", "op": 3 }));
        assert!(invoke_bytes(&[&A, &B], cap_op(2, 0), b"").contains("unknown bulk op"));
        // Kernel codes name no capability, and a load the assembly's
        // capabilities do not read is not answered.
        assert!(invoke_bytes(&[&A], Watch::CODE, b"").contains("unknown bulk op"));
    }

    #[test]
    fn a_load_registers_once_for_every_capability_that_reads_the_kind() {
        let fence = Fence::default();
        let want = crate::canonical_hash(&fence).unwrap();
        // Both lanes, either capability: one identity, one entry.
        let by_json = ok(invoke(&[&A, &B], r#"{"op":"load_fence","fence":{}}"#));
        assert_eq!(by_json, serde_json::json!({ "op": "load_fence", "fence_hash": want }));
        let by_bytes = ok(invoke_bytes(&[&A, &B], Fence::CODE, b"{}"));
        assert_eq!(by_bytes["fence_hash"], want.as_str());
        assert!(kind::<Fence>().get(&want).is_some());
        // The kind a capability does not read is not peeled for it.
        assert_eq!(ok(invoke(&[&A], r#"{"op":"load_watch","document":{}}"#))["cap"], "a");
        // A load without its field names the field.
        let missing = invoke(&[&B], r#"{"op":"load_watch","stops":[]}"#);
        assert!(missing.contains("missing field `document`"), "{missing}");
        let about = ok(invoke_bytes(&[&A, &B], OP_ABOUT, b""));
        assert_eq!(about["capabilities"][1], serde_json::json!({ "id": "b", "documents": ["fence", "watch"] }));
        // The registries are listed once per kind, then the derived ones.
        let held: Vec<&str> = registries(&[&A, &B]).iter().map(|r| r.kind()).collect();
        assert_eq!(held, vec!["fence", "watch", "derived"]);
    }

    #[test]
    fn a_derived_entry_is_a_cache_bounded_per_derivation() {
        const SQUARE: Derivation = Derivation { name: "test square", keep: 2 };
        let computed = std::cell::Cell::new(0);
        let square = |n: u64| {
            *derive(&SQUARE, &n.to_string(), &["sha256:input"], || {
                computed.set(computed.get() + 1);
                Ok(n * n)
            })
            .unwrap()
        };
        assert_eq!(square(3), 9);
        assert_eq!(square(3), 9);
        assert_eq!(computed.get(), 1, "found, not computed again");
        square(4);
        square(5);
        assert_eq!(derived_len(&SQUARE), 2, "the derivation keeps its last two");
        assert_eq!(square(3), 9, "dropped, computed again — the same answer");
        assert_eq!(computed.get(), 4);
        // A keep-set without its input drops it.
        DERIVED.evict(&std::collections::BTreeSet::new());
        assert_eq!(derived_len(&SQUARE), 0);
        // A failed computation holds nothing.
        assert!(derive::<u64>(&SQUARE, "x", &[], || Err("no".into())).is_err());
        assert_eq!(derived_len(&SQUARE), 0);
    }

    // A copy read one version ahead (D-186): held under the identity it
    // is stored with, and a read-only one refuses a write.
    transit_version::kind_tag!(RatedKind = "rated");

    /// v1 of a fixture kind whose v2 adds stars: read ahead, read-only.
    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Rated {
        #[serde(default)]
        kind: RatedKind,
        schema_version: u32,
        name: String,
    }

    impl transit_version::Versioned for Rated {
        const KIND: &'static str = "rated";
        const READS: u32 = 1;
        const AHEAD: Option<(u32, transit_version::Ahead)> = Some((2, transit_version::Ahead::ReadOnly));
        fn schema_version(&self) -> u32 {
            self.schema_version
        }
        fn step_down(mut next: serde_json::Value) -> Result<serde_json::Value, String> {
            let object = next.as_object_mut().ok_or("an object")?;
            object.remove("stars");
            object.insert("schema_version".into(), serde_json::json!(1));
            Ok(next)
        }
    }

    #[derive(serde::Serialize)]
    struct RatedLoaded {
        rated_hash: String,
    }
    impl Load for Rated {
        const ID: &'static str = "rated";
        const NAME: &'static str = "rated";
        const LOAD_OP: &'static str = "load_rated";
        const CODE: u32 = 902;
        const FORM: Form = Form::Json { field: "rated" };
        type Answer = RatedLoaded;
        fn parse(bytes: &[u8]) -> Result<Self, String> {
            crate::parse_versioned(bytes)
        }
        fn canonical(&self) -> Option<Result<String, String>> {
            Some(crate::canonical_hash(self))
        }
        fn answer(&self, rated_hash: String) -> RatedLoaded {
            RatedLoaded { rated_hash }
        }
    }

    #[test]
    fn a_copy_read_ahead_keeps_its_identity_and_a_read_only_one_refuses_a_write() {
        let v2 = serde_json::json!({"kind": "rated", "schema_version": 2, "name": "a", "stars": 4});
        let rated = kind::<Rated>();
        let (hash, _, _) = rated.load(&serde_json::to_vec(&v2).unwrap()).unwrap();
        assert_eq!(hash, crate::canonical_hash(&v2).unwrap(), "the identity it is stored under");
        assert_eq!(rated.require(&hash).unwrap().name, "a", "held stepped down, and read");
        assert_eq!(rated.read_ahead(&hash), Some((2, transit_version::Ahead::ReadOnly)));
        let refused = rated.require_writable(&hash).unwrap_err();
        assert!(refused.contains("rated v2") && refused.contains("does not write over it"), "{refused}");

        let v1 = serde_json::json!({"kind": "rated", "schema_version": 1, "name": "b"});
        let (hash, _, _) = rated.load(&serde_json::to_vec(&v1).unwrap()).unwrap();
        assert!(rated.require_writable(&hash).is_ok(), "a copy in this version is a base as usual");
        assert_eq!(rated.read_ahead(&hash), None);
    }
}
