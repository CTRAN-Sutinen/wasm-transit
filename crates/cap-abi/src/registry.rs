//! A document registry holds documents by content hash. A derived registry
//! caches values that a module computed from documents. It accepts any key
//! and remembers the hashes that the value came from.
//!
//! The host owns the documents and decides which ones a session needs. A
//! host that asks for an evicted document gets the unknown-hash error
//! ([`Registry::require`]) and registers the document again.
//!
//! Only the kernel constructs registries ([`crate::kernel`]), so no
//! capability can declare shared state of its own.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The bytes are `{"keep": ["sha256:…", …]}`. A document survives when its
/// hash is in the keep list. A derived entry survives when every hash that
/// it came from is in the keep list. The answer is [`Evicted`].
pub const OP_EVICT: u32 = 257;

/// The bytes are ignored. The answer is [`Holding`].
pub const OP_HELD: u32 = 258;

/// How a registry bounds itself when the host gives no keep list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retain {
    /// Every entry stays until the host's keep list drops it. Durable kinds
    /// use this.
    Host,
    /// The last N registrations stay, so a poll loop plateaus with no action
    /// from the host. Kinds that every poll makes again use this.
    Last(usize),
}

struct Entry<T: ?Sized> {
    /// The registration order, which `Retain::Last` uses as the age.
    seq: u64,
    /// Empty for a document.
    inputs: Vec<String>,
    value: Arc<T>,
}

struct Entries<T: ?Sized> {
    next: u64,
    map: BTreeMap<String, Entry<T>>,
}

/// A capability that declares a registry of its own does not compile (D-77):
///
/// ```compile_fail
/// use cap_abi::registry::{Registry, Retain};
/// static NETWORKS: Registry<u32> = Registry::documents("network", "load_network", Retain::Host);
/// ```
///
/// ```compile_fail
/// use cap_abi::registry::{Registry, Retain};
/// static GRAPHS: Registry<u32> = Registry::derived("graph", Retain::Last(8));
/// ```
pub struct Registry<T: ?Sized> {
    kind: &'static str,
    /// Empty for a derived registry.
    load_op: &'static str,
    derived: bool,
    retain: Retain,
    entries: Mutex<Entries<T>>,
    ahead: Mutex<BTreeMap<String, (u32, transit_version::Ahead)>>,
}

impl<T: ?Sized> Registry<T> {
    pub(crate) const fn documents(kind: &'static str, load_op: &'static str, retain: Retain) -> Self {
        Registry { kind, load_op, derived: false, retain, entries: Mutex::new(Entries { next: 0, map: BTreeMap::new() }), ahead: Mutex::new(BTreeMap::new()) }
    }

    pub(crate) const fn derived(kind: &'static str, retain: Retain) -> Self {
        Registry { kind, load_op: "", derived: true, retain, entries: Mutex::new(Entries { next: 0, map: BTreeMap::new() }), ahead: Mutex::new(BTreeMap::new()) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Entries<T>> {
        // A panic in the middle of an op leaves nothing that the registry
        // must refuse, because it inserts entries as whole values.
        self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn mark_ahead(&self, hash: &str, found: u32, how: transit_version::Ahead) {
        self.ahead.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(hash.to_string(), (found, how));
    }

    pub(crate) fn ahead(&self, hash: &str) -> Option<(u32, transit_version::Ahead)> {
        self.ahead.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(hash).copied()
    }

    pub fn get(&self, key: &str) -> Option<Arc<T>> {
        self.lock().map.get(key).map(|entry| entry.value.clone())
    }

    pub fn require(&self, hash: &str) -> Result<Arc<T>, transit_version::Refusal> {
        self.get(hash).ok_or_else(|| transit_version::Refusal::unknown_document(self.kind, hash, self.load_op))
    }

    /// A document that is already held keeps its first copy and its age.
    pub fn insert(&self, hash: String, value: T) -> Arc<T>
    where
        T: Sized,
    {
        self.insert_arc(hash, Vec::new(), Arc::new(value))
    }

    pub fn insert_derived(&self, key: String, inputs: Vec<String>, value: Arc<T>) -> Arc<T> {
        self.insert_arc(key, inputs, value)
    }

    /// Bounds one derivation inside the derived registry. It drops the
    /// oldest entries that `within` approves, until at most `n` stay.
    pub(crate) fn bound(&self, n: usize, within: impl Fn(&str) -> bool) {
        let mut entries = self.lock();
        loop {
            let mut matching: Vec<(u64, String)> =
                entries.map.iter().filter(|(k, _)| within(k)).map(|(k, e)| (e.seq, k.clone())).collect();
            if matching.len() <= n.max(1) {
                break;
            }
            matching.sort();
            entries.map.remove(&matching[0].1);
        }
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

    pub fn retain(&self, keep: impl Fn(&str, &[String]) -> bool) {
        self.lock().map.retain(|key, entry| keep(key, &entry.inputs));
    }

    pub fn len(&self) -> usize {
        self.lock().map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub trait Held: Sync {
    fn kind(&self) -> &'static str;
    /// Oldest first.
    fn keys(&self) -> Vec<String>;
    fn evict(&self, keep: &BTreeSet<String>) -> Vec<String>;
}

impl<T: ?Sized + Send + Sync> Held for Registry<T> {
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

/// The key of a document is its hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct HeldEntry {
    pub kind: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct Keep {
    pub keep: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct Evicted {
    pub dropped: Vec<HeldEntry>,
    pub held: Vec<HeldEntry>,
}

/// Entries are by registry in declaration order, oldest first.
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

/// Return `None` for every other op, which the module's dispatch answers.
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
        assert_eq!(
            docs.require("sha256:a").unwrap_err().code,
            Some(transit_version::RefusalCode::UnknownDocument {
                kind: "network".into(),
                hash: "sha256:a".into(),
                load: "load_network".into()
            })
        );
        assert_eq!(*docs.require("sha256:b").unwrap(), 2);
    }

    #[test]
    fn registration_is_idempotent_and_keeps_the_first_copy() {
        let docs: Registry<u32> = Registry::documents("live document", "load_live", Retain::Last(2));
        docs.insert("x".into(), 1);
        docs.insert("y".into(), 2);
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
        assert!(kernel(256, b"", registries).is_none());
        let held = kernel(OP_HELD, b"", registries).unwrap();
        assert_eq!(held, r#"{"ok":{"held":[{"kind":"network","key":"n1"},{"kind":"network","key":"n2"},{"kind":"edits","key":"e1"}]}}"#);
        let evicted = kernel(OP_EVICT, br#"{"keep":["n2"]}"#, registries).unwrap();
        assert_eq!(
            evicted,
            r#"{"ok":{"dropped":[{"kind":"network","key":"n1"},{"kind":"edits","key":"e1"}],"held":[{"kind":"network","key":"n2"}]}}"#
        );
        assert!(kernel(OP_EVICT, b"not json", registries).unwrap().starts_with(r#"{"error":{"message":"invalid request"#));
    }

    proptest! {
        /// However many registrations a poll loop makes, the registry holds
        /// at most N, and they are the N most recent.
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
                while recent.len() > n {
                    recent.remove(0);
                }
                prop_assert!(docs.len() <= n);
                prop_assert_eq!(docs.keys(), recent.clone());
            }
        }

        /// Eviction removes the documents outside the keep list and no others.
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
