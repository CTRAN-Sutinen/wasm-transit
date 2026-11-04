//! The registry: the one type every module keeps its documents in.
//!
//! A module is the hash authority for what it holds, so only the module
//! can drop a document; the host owns the documents, so the module never
//! decides alone what a session still needs. The registry joins the two:
//!
//! * a **document registry** holds documents by their content hash. Its
//!   kind's default says how it is bounded on its own: a durable kind
//!   (networks, feeds, edits, layers, walk tables) keeps everything until
//!   the host says otherwise ([`Retain::Host`]); an ephemeral kind (rt
//!   snapshots, live documents — re-minted every poll) keeps its last N
//!   ([`Retain::Last`]), so a poll loop plateaus with no host steering.
//! * a **derived registry** caches what a module computed from documents
//!   (the router's RAPTOR graph per day), keyed by anything, and
//!   remembers the hashes it was computed from.
//!
//! The kernel ops ([`OP_EVICT`], [`OP_HELD`]) are answered by
//! [`kernel`] over a module's declared registries — the `capability!`
//! macro wires it, so a module declares its registries and never
//! re-implements eviction. An evicted document asked for again is the
//! honest unknown-hash error ([`Registry::require`]), never a wrong
//! answer: the host registers it again.
//!
//! Registries are `const`-constructed statics (no lazy init):
//!
//! ```ignore
//! static NETWORKS: Registry<Network> = Registry::documents("network", "load_network", Retain::Host);
//! static LIVES: Registry<LiveDay> = Registry::documents("live document", "load_live", Retain::Last(4));
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Kernel bulk op: drop everything not named. The bytes are
/// `{"keep": ["sha256:…", …]}`; a document survives when its hash is
/// kept, a derived entry when every hash it was computed from is. The
/// answer is [`Evicted`].
pub const OP_EVICT: u32 = 257;

/// Kernel bulk op: what the module holds, per registry (the bytes are
/// ignored). The answer is [`Holding`].
pub const OP_HELD: u32 = 258;

/// How a registry bounds itself with no host steering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retain {
    /// Durable: everything stays until the host's keep-set drops it.
    Host,
    /// Ephemeral: the most recent N registrations stay; registering
    /// one more drops the oldest.
    Last(usize),
}

struct Entry<T> {
    /// Registration order — the age `Retain::Last` drops by.
    seq: u64,
    /// The document hashes a derived entry was computed from (empty
    /// for a document: its key is its hash).
    inputs: Vec<String>,
    value: Arc<T>,
}

struct Entries<T> {
    next: u64,
    map: BTreeMap<String, Entry<T>>,
}

/// One kind's registry: values by key, bounded by its [`Retain`].
pub struct Registry<T> {
    kind: &'static str,
    /// The op that registers this kind ("load_network") — the
    /// unknown-hash error names it. Empty for a derived registry.
    load_op: &'static str,
    derived: bool,
    retain: Retain,
    entries: Mutex<Entries<T>>,
}

impl<T> Registry<T> {
    /// A document registry: values by their content hash.
    pub const fn documents(kind: &'static str, load_op: &'static str, retain: Retain) -> Self {
        Registry { kind, load_op, derived: false, retain, entries: Mutex::new(Entries { next: 0, map: BTreeMap::new() }) }
    }

    /// A derived registry: values computed from documents, by any key.
    pub const fn derived(kind: &'static str, retain: Retain) -> Self {
        Registry { kind, load_op: "", derived: true, retain, entries: Mutex::new(Entries { next: 0, map: BTreeMap::new() }) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Entries<T>> {
        // A panic mid-op poisons nothing worth refusing: entries are
        // whole values, inserted atomically.
        self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The value under `key`, if held.
    pub fn get(&self, key: &str) -> Option<Arc<T>> {
        self.lock().map.get(key).map(|entry| entry.value.clone())
    }

    /// The document under `hash`, or the unknown-hash error every module
    /// speaks: `unknown <kind> <hash>: <load op> first`. Hosts recover
    /// by registering it again.
    pub fn require(&self, hash: &str) -> Result<Arc<T>, String> {
        self.get(hash).ok_or_else(|| format!("unknown {} {hash}: {} first", self.kind, self.load_op))
    }

    /// Register a document under its hash — idempotent: a document held
    /// already keeps its first copy (and its age).
    pub fn insert(&self, hash: String, value: T) -> Arc<T> {
        self.insert_arc(hash, Vec::new(), Arc::new(value))
    }

    /// Cache a derived value under `key`, computed from `inputs`.
    pub fn insert_derived(&self, key: String, inputs: Vec<String>, value: Arc<T>) -> Arc<T> {
        self.insert_arc(key, inputs, value)
    }

    fn insert_arc(&self, key: String, inputs: Vec<String>, value: Arc<T>) -> Arc<T> {
        let mut entries = self.lock();
        if let Some(held) = entries.map.get(&key) {
            return held.value.clone();
        }
        let seq = entries.next;
        entries.next += 1;
        entries.map.insert(key, Entry { seq, inputs, value: value.clone() });
        if let Retain::Last(n) = self.retain {
            while entries.map.len() > n.max(1) {
                let oldest = entries.map.iter().min_by_key(|(_, e)| e.seq).map(|(k, _)| k.clone());
                match oldest {
                    Some(key) => entries.map.remove(&key),
                    None => break,
                };
            }
        }
        value
    }

    /// Keep only the entries `keep(key, inputs)` approves — a module's
    /// own rule over its derived cache (the router keeps only the
    /// newest live document's graphs).
    pub fn retain(&self, keep: impl Fn(&str, &[String]) -> bool) {
        self.lock().map.retain(|key, entry| keep(key, &entry.inputs));
    }

    /// How many values are held.
    pub fn len(&self) -> usize {
        self.lock().map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A registry seen through the kernel ops, whatever it holds.
pub trait Held: Sync {
    fn kind(&self) -> &'static str;
    /// The keys held, oldest first.
    fn keys(&self) -> Vec<String>;
    /// Drop every entry the keep-set does not cover; answer the keys
    /// dropped.
    fn evict(&self, keep: &BTreeSet<String>) -> Vec<String>;
}

impl<T: Send + Sync> Held for Registry<T> {
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn keys(&self) -> Vec<String> {
        let entries = self.lock();
        let mut keys: Vec<(&u64, &String)> = entries.map.iter().map(|(k, e)| (&e.seq, k)).collect();
        keys.sort();
        keys.into_iter().map(|(_, k)| k.clone()).collect()
    }

    fn evict(&self, keep: &BTreeSet<String>) -> Vec<String> {
        let mut entries = self.lock();
        let derived = self.derived;
        let dropped: Vec<String> = entries
            .map
            .iter()
            .filter(|(key, entry)| {
                if derived {
                    !entry.inputs.iter().all(|input| keep.contains(input))
                } else {
                    !keep.contains(*key)
                }
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in &dropped {
            entries.map.remove(key);
        }
        dropped
    }
}

/// One held entry: its registry's kind and its key (a document's hash).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct HeldEntry {
    pub kind: String,
    pub key: String,
}

/// [`OP_EVICT`]'s request.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct Keep {
    pub keep: Vec<String>,
}

/// [`OP_EVICT`]'s answer: what was dropped, and what is held after.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct Evicted {
    pub dropped: Vec<HeldEntry>,
    pub held: Vec<HeldEntry>,
}

/// [`OP_HELD`]'s answer: every entry held, per registry in declaration
/// order, oldest first.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct Holding {
    pub held: Vec<HeldEntry>,
}

fn holding(registries: &[&dyn Held]) -> Vec<HeldEntry> {
    registries
        .iter()
        .flat_map(|r| r.keys().into_iter().map(|key| HeldEntry { kind: r.kind().to_string(), key }))
        .collect()
}

/// The kernel ops over a module's registries: `Some(outcome)` for a
/// kernel op this answers, `None` for every other op (the module's
/// own dispatch answers it).
pub fn kernel(op: u32, bytes: &[u8], registries: &[&dyn Held]) -> Option<String> {
    match op {
        OP_EVICT => Some(crate::outcome(
            serde_json::from_slice::<Keep>(bytes)
                .map_err(|e| format!("invalid request: {e}"))
                .map(|request| {
                    let keep: BTreeSet<String> = request.keep.into_iter().collect();
                    let dropped = registries
                        .iter()
                        .flat_map(|r| r.evict(&keep).into_iter().map(|key| HeldEntry { kind: r.kind().to_string(), key }))
                        .collect();
                    Evicted { dropped, held: holding(registries) }
                }),
        )),
        OP_HELD => Some(crate::outcome(Ok::<_, String>(Holding { held: holding(registries) }))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn keep(hashes: &[&str]) -> BTreeSet<String> {
        hashes.iter().map(|h| h.to_string()).collect()
    }

    #[test]
    fn evicted_then_asked_is_the_unknown_hash_error() {
        let docs: Registry<u32> = Registry::documents("network", "load_network", Retain::Host);
        docs.insert("sha256:a".into(), 1);
        docs.insert("sha256:b".into(), 2);
        assert_eq!(docs.evict(&keep(&["sha256:b"])), vec!["sha256:a".to_string()]);
        assert_eq!(docs.require("sha256:a").unwrap_err(), "unknown network sha256:a: load_network first");
        assert_eq!(*docs.require("sha256:b").unwrap(), 2);
    }

    #[test]
    fn registration_is_idempotent_and_keeps_the_first_copy() {
        let docs: Registry<u32> = Registry::documents("live document", "load_live", Retain::Last(2));
        docs.insert("x".into(), 1);
        docs.insert("y".into(), 2);
        // Re-registering x neither replaces it nor makes it younger.
        assert_eq!(*docs.insert("x".into(), 9), 1);
        docs.insert("z".into(), 3);
        assert!(docs.get("x").is_none(), "x was the oldest");
        assert_eq!(docs.keys(), vec!["y".to_string(), "z".to_string()]);
    }

    #[test]
    fn a_derived_entry_survives_only_while_all_its_inputs_are_kept() {
        let graphs: Registry<u32> = Registry::derived("graph", Retain::Host);
        graphs.insert_derived("n|e|d".into(), vec!["n".into(), "e".into()], Arc::new(1));
        graphs.insert_derived("n|d".into(), vec!["n".into()], Arc::new(2));
        assert_eq!(graphs.evict(&keep(&["n"])), vec!["n|e|d".to_string()]);
        assert_eq!(graphs.len(), 1);
    }

    #[test]
    fn the_kernel_answers_evict_and_held_and_passes_everything_else() {
        static A: Registry<u32> = Registry::documents("network", "load_network", Retain::Host);
        static B: Registry<u32> = Registry::documents("edits", "load_edits", Retain::Host);
        A.insert("n1".into(), 1);
        A.insert("n2".into(), 2);
        B.insert("e1".into(), 3);
        let registries: &[&dyn Held] = &[&A, &B];
        assert!(kernel(0, b"", registries).is_none());
        assert!(kernel(crate::OP_LOAD_NETWORK, b"", registries).is_none());
        let held = kernel(OP_HELD, b"", registries).unwrap();
        assert_eq!(held, r#"{"ok":{"held":[{"kind":"network","key":"n1"},{"kind":"network","key":"n2"},{"kind":"edits","key":"e1"}]}}"#);
        let evicted = kernel(OP_EVICT, br#"{"keep":["n2"]}"#, registries).unwrap();
        assert_eq!(
            evicted,
            r#"{"ok":{"dropped":[{"kind":"network","key":"n1"},{"kind":"edits","key":"e1"}],"held":[{"kind":"network","key":"n2"}]}}"#
        );
        assert!(kernel(OP_EVICT, b"not json", registries).unwrap().starts_with(r#"{"error":"invalid request"#));
    }

    proptest! {
        /// Keep-last-N plateaus: however many registrations a poll loop
        /// makes, at most N are held, and they are the N most recent.
        #[test]
        fn keep_last_n_plateaus(n in 1usize..6, hashes in proptest::collection::vec(0u8..40, 0..200)) {
            let docs: Registry<u8> = Registry::documents("rt snapshot", "decode", Retain::Last(n));
            let mut recent: Vec<String> = Vec::new();
            for h in hashes {
                let key = format!("sha256:{h}");
                docs.insert(key.clone(), h);
                if !recent.contains(&key) {
                    recent.push(key);
                }
                // The model: first registrations in order, the oldest dropped.
                while recent.len() > n {
                    recent.remove(0);
                }
                prop_assert!(docs.len() <= n);
                prop_assert_eq!(docs.keys(), recent.clone());
            }
        }

        /// A keep-set entry never drops: eviction removes exactly the
        /// documents outside it.
        #[test]
        fn a_kept_document_never_drops(
            held in proptest::collection::btree_set(0u8..30, 0..20),
            kept in proptest::collection::btree_set(0u8..30, 0..20),
        ) {
            let docs: Registry<u8> = Registry::documents("network", "load_network", Retain::Host);
            for h in &held {
                docs.insert(format!("h{h}"), *h);
            }
            let keep: BTreeSet<String> = kept.iter().map(|k| format!("h{k}")).collect();
            let dropped = docs.evict(&keep);
            for h in &held {
                let key = format!("h{h}");
                prop_assert_eq!(docs.get(&key).is_some(), keep.contains(&key));
                prop_assert_eq!(dropped.contains(&key), !keep.contains(&key));
            }
        }
    }
}
