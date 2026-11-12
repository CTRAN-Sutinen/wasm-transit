//! The kernel: the registries every capability reads, the loads that
//! fill them, and the routing an assembly's four exports run.
//!
//! **One registry per document kind, owned here.** A capability names
//! the kinds it reads (`capability! { documents: [NETWORK, EDITS] }`)
//! and reads them through the kind ([`Kind::require`]); it cannot
//! declare a registry of its own (the constructor is private to this
//! crate). Linked into one assembly, every capability reads the same
//! `Arc`: a document registers once per instance, however many
//! capabilities read it (D-77).
//!
//! **Registration is the kernel's.** Each kind's load is a kernel op —
//! a bulk op from 259 (the network's is 256), and the same JSON op
//! (`{"op": "load_network", "network": …}`) on the JSON lane, peeled
//! before any capability decodes the request. A load answers the
//! document's identity (`<kind>_hash`, the module is the hash
//! authority) and what the host shows about it (a feed's summary, a
//! layer's fields). A capability's `Request` carries no load op.
//!
//! **Derived data is held here too.** A derivation — the day patched
//! with the edits in force, the router's graph, a street graph — is a
//! library's pure function and its key; [`derive`] finds the entry or
//! computes it, bounded per [`Derivation`], and drops it when a
//! keep-set drops any document it was computed from. The answer is
//! the same either way: an entry is a cache, never a dependency
//! (assembly independence, `crates/cap-abi/independence.mjs`).
//!
//! **Routing.** An assembly is one or more [`Cap`]s. A JSON request
//! names its capability (`{"cap": "board", "op": …}`) unless the
//! assembly holds one; a capability's bulk op carries its index in
//! the high 16 bits (its own codes are 0–255); the kernel ops (256 and
//! up) name none. [`invoke`] and [`invoke_bytes`] are what the
//! exports run (`assembly!`), and what native tests call.

use std::any::Any;
use std::borrow::Cow;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use ts_rs::TS;

use crate::registry::{Held, Registry, Retain, OP_EVICT, OP_HELD};

pub use transit_analysis::WeightedLayer;
pub use transit_codec::{Boundary, Feed, FeedSummary};
pub use transit_live::LiveDay;
pub use transit_network::Network;
pub use transit_rt::RtOverview;
pub use transit_streets::Streets;
pub use transit_streets::walking::WalkTable;
pub use transit_sync::{EditDocument, EditRecord};
pub use transit_dispatch::watchlist::Watchlist;
pub use transit_runs::WorkRules;
pub use transit_stops::Stops;

// ---------------------------------------------------------------------
// the kernel ops
// ---------------------------------------------------------------------

/// The load ops' bulk codes. The network's is 256 ([`crate::OP_LOAD_NETWORK`]);
/// the registry ops are 257 and 258.
pub const OP_LOAD_EDITS: u32 = 259;
pub const OP_LOAD_LIVE: u32 = 260;
pub const OP_LOAD_BOUNDARY: u32 = 261;
pub const OP_LOAD_WALKS: u32 = 262;
pub const OP_LOAD_STREETS: u32 = 263;
pub const OP_LOAD_WATCHLIST: u32 = 264;
pub const OP_LOAD_LAYER: u32 = 265;
pub const OP_LOAD_FEED: u32 = 266;
pub const OP_LOAD_RT: u32 = 267;

/// Kernel bulk op: what the module is — the capabilities it links, in
/// index order, each with the document kinds it reads (the bytes are
/// ignored). The answer is [`About`].
pub const OP_ABOUT: u32 = 268;

/// The work rules' load (after [`OP_ABOUT`], which took 268).
pub const OP_LOAD_WORK_RULES: u32 = 269;

/// The stops document's load.
pub const OP_LOAD_STOPS: u32 = 270;

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

/// What a load answers: the document's identity under its kind's
/// field, and what the host shows about it. Tagged like every op's
/// answer.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_abi/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Loaded {
    LoadNetwork { network_hash: String },
    LoadEdits { edits_hash: String },
    LoadLive { live_hash: String },
    LoadBoundary { boundary_hash: String },
    LoadWalks { walks_hash: String },
    /// The document's size as its graph will be: its streets, their
    /// segments, and their distinct points.
    LoadStreets { streets_hash: String, ways: u32, segments: u32, nodes: u32 },
    LoadWatchlist { watchlist_hash: String, version: u32 },
    /// What the layer holds and the weights it can be read by (each
    /// numeric property, with its total) — the whole answer the
    /// layer's import shows; a coverage op names the field.
    LoadLayer {
        layer_hash: String,
        #[ts(type = "number")]
        features: u64,
        #[ts(type = "number")]
        points: u64,
        #[ts(type = "number")]
        polygons: u64,
        fields: Vec<transit_analysis::LayerField>,
    },
    LoadFeed { feed_hash: String, summary: FeedSummary },
    LoadRt { rt_hash: String },
    /// The version the rules speak.
    LoadWorkRules { work_rules_hash: String, version: u32 },
    /// How many stops the document holds.
    LoadStops { stops_hash: String, stops: u32 },
}

/// The JSON lane's envelope: the op, the capability it names, and the
/// fields a load reads — each left as raw JSON until a load asks for
/// it, so a request that is not a load costs one scan.
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow)]
    op: Cow<'a, str>,
    #[serde(borrow, default)]
    cap: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    network: Option<&'a RawValue>,
    #[serde(borrow, default)]
    edits: Option<&'a RawValue>,
    #[serde(borrow, default)]
    live: Option<&'a RawValue>,
    #[serde(borrow, default)]
    boundary: Option<&'a RawValue>,
    #[serde(borrow, default)]
    walks: Option<&'a RawValue>,
    #[serde(borrow, default)]
    streets: Option<&'a RawValue>,
    #[serde(borrow, default)]
    document: Option<&'a RawValue>,
    #[serde(borrow, default)]
    work_rules: Option<&'a RawValue>,
    #[serde(borrow, default)]
    stops: Option<&'a RawValue>,
    #[serde(borrow, default)]
    bytes: Option<&'a RawValue>,
}

impl<'a> Envelope<'a> {
    /// A field the load needs, or the request error naming it.
    fn field(&self, value: Option<&'a RawValue>, name: &str) -> Result<&'a [u8], String> {
        value.map(|raw| raw.get().as_bytes()).ok_or_else(|| format!("invalid request: missing field `{name}`"))
    }

    /// A byte document sent on the JSON lane (an array of numbers).
    fn byte_field(&self) -> Result<Vec<u8>, String> {
        serde_json::from_slice(self.field(self.bytes, "bytes")?).map_err(|e| format!("invalid request: bytes: {e}"))
    }
}

// ---------------------------------------------------------------------
// the kinds
// ---------------------------------------------------------------------

/// How a kind's identity is derived.
enum Identity<T> {
    /// The canonical hash of the parsed document (struct field order):
    /// byte variants of one document are one document.
    Canonical(fn(&T) -> Result<String, String>),
    /// The content hash of the bytes as they crossed (a zip, a
    /// protobuf, a GeoJSON layer — the bytes ARE the document).
    Bytes,
}

/// One document kind: its registry and its load.
pub struct Kind<T: 'static> {
    /// The kind's id, as the manifest's `documents` names it.
    pub id: &'static str,
    /// The registry's kind, as `held` and the unknown-hash error name it.
    pub name: &'static str,
    /// The JSON op that registers it.
    pub load_op: &'static str,
    /// The bulk op that registers it.
    pub op: u32,
    identity: Identity<T>,
    registry: Registry<T>,
    /// The document from bulk bytes.
    parse: fn(&[u8]) -> Result<T, String>,
    /// The document's bytes from the JSON lane's envelope.
    json: for<'a> fn(&Envelope<'a>) -> Result<Cow<'a, [u8]>, String>,
    answer: fn(String, &T) -> Loaded,
}

impl<T: Send + Sync + 'static> Kind<T> {
    /// The document under `hash`, or the unknown-hash error every
    /// module speaks (`unknown <kind> <hash>: <load op> first`).
    pub fn require(&self, hash: &str) -> Result<Arc<T>, String> {
        self.registry.require(hash)
    }

    /// The document under `hash`, if held.
    pub fn get(&self, hash: &str) -> Option<Arc<T>> {
        self.registry.get(hash)
    }

    /// Register a document an op minted (an import's network, cap-rt's
    /// live document) and answer its identity: the page holds a handle,
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
        let held = self.registry.insert(hash.clone(), (self.parse)(bytes)?);
        Ok((hash, held))
    }

    /// How many documents of the kind are held.
    pub fn len(&self) -> usize {
        self.registry.len()
    }

    pub fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }

    fn load(&self, bytes: &[u8]) -> Result<Loaded, String> {
        let (hash, held) = match self.identity {
            Identity::Canonical(identity) => {
                let document = (self.parse)(bytes)?;
                let hash = identity(&document)?;
                (hash.clone(), self.registry.insert(hash, document))
            }
            Identity::Bytes => self.register_bytes(bytes)?,
        };
        Ok((self.answer)(hash, &held))
    }
}

/// A kind seen by the routing, whatever it holds.
pub trait Load: Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn load_op(&self) -> &'static str;
    fn op(&self) -> u32;
    fn held(&self) -> &dyn Held;
    fn load_bytes(&self, bytes: &[u8]) -> String;
    fn load_json(&self, envelope: &Envelope) -> String;
}

impl<T: Send + Sync + 'static> Load for Kind<T> {
    fn id(&self) -> &'static str {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn load_op(&self) -> &'static str {
        self.load_op
    }
    fn op(&self) -> u32 {
        self.op
    }
    fn held(&self) -> &dyn Held {
        &self.registry
    }
    fn load_bytes(&self, bytes: &[u8]) -> String {
        crate::outcome(self.load(bytes))
    }
    fn load_json(&self, envelope: &Envelope) -> String {
        crate::outcome((self.json)(envelope).and_then(|bytes| self.load(&bytes)))
    }
}

fn canonical<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    crate::parse_document(bytes)
}

/// Networks: the document every model-side capability reads.
pub static NETWORK: Kind<Network> = Kind {
    id: "network",
    name: "network",
    load_op: "load_network",
    op: crate::OP_LOAD_NETWORK,
    identity: Identity::Canonical(crate::canonical_hash),
    registry: Registry::documents("network", "load_network", Retain::Host),
    parse: |bytes| crate::parse_network(bytes),
    json: |e| e.field(e.network, "network").map(Cow::Borrowed),
    answer: |network_hash, _| Loaded::LoadNetwork { network_hash },
};

/// Operator edit documents (the tagged document, wholesale-replaced
/// by the host), in the version this build reads: an older or newer
/// one is refused at the load, naming both versions. Validation
/// against a network is a question (cap-divert `reports`), never the
/// load's.
pub static EDITS: Kind<EditDocument> = Kind {
    id: "edits",
    name: "edits",
    load_op: "load_edits",
    op: OP_LOAD_EDITS,
    identity: Identity::Canonical(crate::canonical_hash),
    registry: Registry::documents("edits", "load_edits", Retain::Host),
    parse: crate::parse_versioned,
    json: |e| e.field(e.edits, "edits").map(Cow::Borrowed),
    answer: |edits_hash, _| Loaded::LoadEdits { edits_hash },
};

/// Live documents — ephemeral: re-minted every poll, the last four
/// kept (D-70). Like every versioned kind below, refused at the load
/// in another version, naming both (`crate::parse_versioned`).
pub static LIVE: Kind<LiveDay> = Kind {
    id: "live",
    name: "live document",
    load_op: "load_live",
    op: OP_LOAD_LIVE,
    identity: Identity::Canonical(crate::canonical_hash),
    registry: Registry::documents("live document", "load_live", Retain::Last(4)),
    parse: crate::parse_versioned,
    json: |e| e.field(e.live, "live").map(Cow::Borrowed),
    answer: |live_hash, _| Loaded::LoadLive { live_hash },
};

/// An import's boundary (the feed numbering an rt join pairs by).
pub static BOUNDARY: Kind<Boundary> = Kind {
    id: "boundary",
    name: "boundary",
    load_op: "load_boundary",
    op: OP_LOAD_BOUNDARY,
    identity: Identity::Canonical(crate::canonical_hash),
    registry: Registry::documents("boundary", "load_boundary", Retain::Host),
    parse: crate::parse_versioned,
    json: |e| e.field(e.boundary, "boundary").map(Cow::Borrowed),
    answer: |boundary_hash, _| Loaded::LoadBoundary { boundary_hash },
};

/// Walk tables (cap-streets `walks` mints them).
pub static WALKS: Kind<WalkTable> = Kind {
    id: "walks",
    name: "walk table",
    load_op: "load_walks",
    op: OP_LOAD_WALKS,
    identity: Identity::Canonical(crate::canonical_hash),
    registry: Registry::documents("walk table", "load_walks", Retain::Host),
    parse: crate::parse_versioned,
    json: |e| e.field(e.walks, "walks").map(Cow::Borrowed),
    answer: |walks_hash, _| Loaded::LoadWalks { walks_hash },
};

/// Streets documents, refused unless valid (so a registered document
/// hashes as its import did).
pub static STREETS: Kind<Streets> = Kind {
    id: "streets",
    name: "street network",
    load_op: "load_streets",
    op: OP_LOAD_STREETS,
    identity: Identity::Canonical(crate::canonical_hash),
    registry: Registry::documents("street network", "load_streets", Retain::Host),
    parse: |bytes| {
        let streets: Streets = crate::parse_versioned(bytes).map_err(|e| format!("streets document: {e}"))?;
        streets.validate().map_err(|e| format!("streets document: {}", e.0))?;
        Ok(streets)
    },
    json: |e| e.field(e.streets, "streets").map(Cow::Borrowed),
    answer: |streets_hash, streets| {
        let (ways, segments, nodes) = streets.counts();
        Loaded::LoadStreets { streets_hash, ways, segments, nodes }
    },
};

/// Watchlists (the personal saved set, version-tagged).
pub static WATCHLIST: Kind<Watchlist> = Kind {
    id: "watchlist",
    name: "watchlist",
    load_op: "load_watchlist",
    op: OP_LOAD_WATCHLIST,
    identity: Identity::Canonical(crate::canonical_hash),
    registry: Registry::documents("watchlist", "load_watchlist", Retain::Host),
    parse: canonical,
    json: |e| e.field(e.document, "document").map(Cow::Borrowed),
    answer: |watchlist_hash, watchlist| Loaded::LoadWatchlist { watchlist_hash, version: watchlist.version() },
};

/// Work rules (transit-runs): the rules work is cut under, checked
/// and normalized at the load, so two orderings of one document are
/// one identity.
pub static WORK_RULES: Kind<WorkRules> = Kind {
    id: "work_rules",
    name: "work rules",
    load_op: "load_work_rules",
    op: OP_LOAD_WORK_RULES,
    identity: Identity::Canonical(crate::canonical_hash),
    registry: Registry::documents("work rules", "load_work_rules", Retain::Host),
    parse: |bytes| {
        let rules: WorkRules = crate::parse_versioned(bytes).map_err(|e| format!("work rules: {e}"))?;
        rules.checked().map_err(|e| format!("work rules: {e}"))
    },
    json: |e| e.field(e.work_rules, "work_rules").map(Cow::Borrowed),
    answer: |work_rules_hash, rules| Loaded::LoadWorkRules { work_rules_hash, version: rules.version },
};

/// Stops documents (transit-stops: every stop a list of dated events,
/// D-153), refused unless a write could store them — each refusal
/// named — so a registered document is always one the model reads.
pub static STOPS: Kind<Stops> = Kind {
    id: "stops",
    name: "stops document",
    load_op: "load_stops",
    op: OP_LOAD_STOPS,
    identity: Identity::Canonical(crate::canonical_hash),
    registry: Registry::documents("stops document", "load_stops", Retain::Host),
    parse: |bytes| {
        let stops: Stops = crate::parse_versioned(bytes).map_err(|e| format!("stops document: {e}"))?;
        stops.checked().map_err(|refusals| {
            let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
            format!("stops document: {}", reasons.join("; "))
        })
    },
    json: |e| e.field(e.stops, "stops").map(Cow::Borrowed),
    answer: |stops_hash, stops| Loaded::LoadStops { stops_hash, stops: stops.stops.len() as u32 },
};

/// Weighted layers (GeoJSON points and polygons — people, jobs,
/// households; D-47), by the hash of their bytes: which property is
/// the weight is the coverage op's question, so the bytes are the
/// whole document.
pub static LAYER: Kind<WeightedLayer> = Kind {
    id: "layer",
    name: "layer",
    load_op: "load_layer",
    op: OP_LOAD_LAYER,
    identity: Identity::Bytes,
    registry: Registry::documents("layer", "load_layer", Retain::Host),
    parse: transit_analysis::parse_layer,
    json: |e| e.byte_field().map(Cow::Owned),
    answer: |layer_hash, layer| Loaded::LoadLayer {
        layer_hash,
        features: layer.features() as u64,
        points: layer.points() as u64,
        polygons: layer.polygons() as u64,
        fields: layer.fields().to_vec(),
    },
};

/// GTFS feeds (the zip's bytes), by the hash of their bytes.
pub static FEED: Kind<Feed> = Kind {
    id: "feed",
    name: "feed",
    load_op: "load_feed",
    op: OP_LOAD_FEED,
    identity: Identity::Bytes,
    registry: Registry::documents("feed", "load_feed", Retain::Host),
    parse: |bytes| Feed::from_zip_bytes(bytes).map_err(|e| format!("parse: {e}")),
    json: |e| e.byte_field().map(Cow::Owned),
    answer: |feed_hash, feed| Loaded::LoadFeed { feed_hash, summary: feed.summary() },
};

/// Decoded GTFS-rt snapshots, by the hash of the payload — ephemeral:
/// an agency re-mints them every poll, and a live answer reads at most
/// three (trip updates, positions, alerts), so the last eight stay.
pub static RT: Kind<RtOverview> = Kind {
    id: "rt",
    name: "rt snapshot",
    load_op: "load_rt",
    op: OP_LOAD_RT,
    identity: Identity::Bytes,
    registry: Registry::documents("rt snapshot", "load_rt", Retain::Last(8)),
    parse: |bytes| transit_rt::decode_overview(bytes),
    json: |e| e.byte_field().map(Cow::Owned),
    answer: |rt_hash, _| Loaded::LoadRt { rt_hash },
};

// ---------------------------------------------------------------------
// derived entries
// ---------------------------------------------------------------------

/// Every derived entry, whatever it holds, by `<derivation>|<key>`.
static DERIVED: Registry<dyn Any + Send + Sync> = Registry::derived("derived", Retain::Host);

/// The derived registry, as the registry ops see it.
pub fn derived_registry() -> &'static dyn Held {
    &DERIVED
}

/// One derivation: its name (the entry's key prefix) and how many of
/// its entries an instance keeps on its own (D-70).
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
/// from: a keep-set that drops any of them drops the entry. Whoever
/// computes it, the value is the same — a library's pure function of
/// the documents the key names.
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

/// Drop the derivation's entries `keep` refuses — a capability's own
/// rule over its entries (the router keeps only the newest live
/// document's graphs).
pub fn retain_derived(derivation: &Derivation, keep: impl Fn(&str, &[String]) -> bool) {
    let prefix = format!("{}|", derivation.name);
    DERIVED.retain(|key, inputs| match key.strip_prefix(&prefix) {
        Some(rest) => keep(rest, inputs),
        None => true,
    });
}

/// The day for (network, edits, date), held: the effective trips and,
/// when edits are in force, what they took away.
pub fn day(network_hash: &str, edits_hash: Option<&str>, date: &str) -> Result<Arc<transit_dispatch::Day>, String> {
    // The request's own fields validate before any registry lookup —
    // a garbage date is a caller error, not a missing-document error.
    let parsed = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|e| format!("invalid date {date:?}: {e}"))?;
    let network = NETWORK.require(network_hash)?;
    let records = edits_hash.map(|hash| EDITS.require(hash)).transpose()?;
    let mut inputs = vec![network_hash];
    inputs.extend(edits_hash);
    derive(&DAY, &transit_dispatch::day_key(network_hash, edits_hash, date), &inputs, || {
        transit_dispatch::day(&network, parsed, records.as_deref().map(|d| d.records.as_slice()).unwrap_or(&[]))
            .map_err(|e| format!("the network does not materialize: {e}"))
    })
}

// ---------------------------------------------------------------------
// capabilities and assemblies
// ---------------------------------------------------------------------

/// One capability as an assembly links it: its id, its dispatch pair,
/// and the document kinds it reads. `capability!` declares it; an
/// assembly (`assembly!`) routes to it.
pub struct Cap {
    pub id: &'static str,
    pub json: fn(&str) -> String,
    pub bytes: fn(u32, &[u8]) -> String,
    pub documents: &'static [&'static dyn Load],
}

/// The kinds an assembly's capabilities read, each once.
fn kinds<'a>(caps: &[&'a Cap]) -> Vec<&'a dyn Load> {
    let mut out: Vec<&dyn Load> = Vec::new();
    for cap in caps {
        for kind in cap.documents {
            if !out.iter().any(|k| k.name() == kind.name()) {
                out.push(*kind);
            }
        }
    }
    out
}

/// The registries an assembly holds, in the order `held` lists them:
/// each kind its capabilities read, then the derived entries.
pub fn registries<'a>(caps: &[&'a Cap]) -> Vec<&'a dyn Held> {
    let mut out: Vec<&dyn Held> = kinds(caps).into_iter().map(|k| k.held()).collect();
    out.push(&DERIVED);
    out
}

fn error(message: String) -> String {
    crate::outcome::<()>(Err(message))
}

/// The JSON lane of an assembly: a kernel load when the op is one of
/// its kinds' loads, otherwise the capability the request names (or
/// the only one).
pub fn invoke(caps: &[&Cap], text: &str) -> String {
    let envelope: Option<Envelope> = serde_json::from_str(text).ok();
    if let Some(envelope) = &envelope {
        if let Some(kind) = kinds(caps).into_iter().find(|k| k.load_op() == envelope.op) {
            return kind.load_json(envelope);
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

/// The bulk lane of an assembly: the registry ops, its kinds' loads,
/// and each capability's own bulk ops (its index in the high 16 bits,
/// its code in the low 8).
pub fn invoke_bytes(caps: &[&Cap], op: u32, bytes: &[u8]) -> String {
    if op == OP_EVICT || op == OP_HELD {
        return crate::registry::kernel(op, bytes, &registries(caps)).expect("evict and held are the registry ops");
    }
    if op == OP_ABOUT {
        let capabilities = caps
            .iter()
            .map(|c| AboutCap { id: c.id.to_string(), documents: c.documents.iter().map(|k| k.id().to_string()).collect() })
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
    static A: Cap = Cap { id: "a", json: echo_a, bytes: bytes_a, documents: &[&BOUNDARY] };
    static B: Cap = Cap { id: "b", json: echo_b, bytes: bytes_b, documents: &[&BOUNDARY, &WATCHLIST] };

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
        assert!(invoke_bytes(&[&A, &B], OP_LOAD_FEED, b"").contains("unknown bulk op"));
    }

    #[test]
    fn a_load_registers_once_for_every_capability_that_reads_the_kind() {
        let boundary = Boundary::default();
        let want = crate::canonical_hash(&boundary).unwrap();
        // Both lanes, either capability: one identity, one entry.
        let by_json = ok(invoke(&[&A, &B], r#"{"op":"load_boundary","boundary":{}}"#));
        assert_eq!(by_json, serde_json::json!({ "op": "load_boundary", "boundary_hash": want }));
        let by_bytes = ok(invoke_bytes(&[&A, &B], OP_LOAD_BOUNDARY, b"{}"));
        assert_eq!(by_bytes["boundary_hash"], want.as_str());
        assert!(BOUNDARY.get(&want).is_some());
        // The kind a capability does not read is not peeled for it.
        assert_eq!(ok(invoke(&[&A], r#"{"op":"load_watchlist","document":{}}"#))["cap"], "a");
        let about = ok(invoke_bytes(&[&A, &B], OP_ABOUT, b""));
        assert_eq!(about["capabilities"][1], serde_json::json!({ "id": "b", "documents": ["boundary", "watchlist"] }));
        // The registries are listed once per kind, then the derived ones.
        let held: Vec<&str> = registries(&[&A, &B]).iter().map(|r| r.kind()).collect();
        assert_eq!(held, vec!["boundary", "watchlist", "derived"]);
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
}
