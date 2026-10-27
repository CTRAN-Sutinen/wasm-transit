//! Assembly validation: the COMPOSE-time tier of the proof stack.
//!
//! The registry's rigor is layered, and each layer is cheap where it
//! runs:
//!
//! * **Build time** (expensive, once, published as evidence): the
//!   property laws with independent oracles, the module dispatch
//!   tests, the mutation runs. An agent never re-runs these — it
//!   reads their attestations.
//! * **Compose time** (this module — light, cheap, TOTAL): the
//!   assembly a agent writes is checked the way a compiler
//!   type-checks, not the way a test suite samples. Every check is
//!   decidable over the manifest alone, holds for ALL future requests
//!   rather than worked examples, and finishes in microseconds.
//! * **Run time** (the seam): fetched bytes verified against pinned
//!   hashes, documents rejected at load when out-of-version, every op
//!   total (errors, never panics — the panic-fuzz law).
//!
//! What "total" buys: a validated assembly cannot later discover a
//! document some loaded capability can neither speak nor migrate —
//! version coherence is proven for every kind, not demonstrated on
//! the documents that happened to be lying around.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::manifest::{CapManifest, Capability};

/// One thing wrong with an assembly, in the vocabulary of the
/// registry (capability id, check name, human-readable detail).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AssemblyIssue {
    pub capability: String,
    pub check: String,
    pub detail: String,
}

/// The kernel the assembling host speaks; a manifest built for a
/// different kernel is rejected at the seam, not mid-flight.
pub const HOST_KERNEL: u32 = 1;

/// Validate an assembly: decidable checks, total over the manifest.
/// Ok(()) means the assembly is provably coherent — dependency
/// closure, document-kind coherence between producers and consumers,
/// and migration-path mutual reachability for every document kind.
pub fn validate_assembly(manifest: &CapManifest) -> Result<(), Vec<AssemblyIssue>> {
    validate_with_kernel(manifest, HOST_KERNEL)
}

pub fn validate_with_kernel(
    manifest: &CapManifest,
    host_kernel: u32,
) -> Result<(), Vec<AssemblyIssue>> {
    let mut issues = Vec::new();
    if manifest.kernel != host_kernel {
        issues.push(AssemblyIssue {
            capability: "(manifest)".into(),
            check: "kernel".into(),
            detail: format!("manifest kernel v{} != host kernel v{host_kernel}", manifest.kernel),
        });
    }

    let by_id: HashMap<&str, &Capability> = manifest
        .capabilities
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();

    // Unique ids, hash pinning, and per-capability shape.
    let mut seen = BTreeSet::new();
    for cap in &manifest.capabilities {
        if !seen.insert(cap.id.as_str()) {
            issues.push(AssemblyIssue {
                capability: cap.id.clone(),
                check: "unique_ids".into(),
                detail: "duplicate capability id".into(),
            });
            continue;
        }
        let hex = cap.hash.strip_prefix("sha256:").unwrap_or("");
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            issues.push(AssemblyIssue {
                capability: cap.id.clone(),
                check: "hash_format".into(),
                detail: format!("hash {:?} is not sha256:<64 hex>", cap.hash),
            });
        }
        // The served bytes are pinned BY NAME: the url must carry the
        // hash, or a swap at the url would defeat the pin.
        if !cap.url.contains(hex) {
            issues.push(AssemblyIssue {
                capability: cap.id.clone(),
                check: "hash_pinned_url".into(),
                detail: format!("url {:?} does not carry the pinned hash", cap.url),
            });
        }
        if cap.ops.is_empty() {
            issues.push(AssemblyIssue {
                capability: cap.id.clone(),
                check: "ops".into(),
                detail: "a capability with no ops answers nothing".into(),
            });
        }
        let mut ops = BTreeSet::new();
        for op in &cap.ops {
            if !ops.insert(op.as_str()) {
                issues.push(AssemblyIssue {
                    capability: cap.id.clone(),
                    check: "ops".into(),
                    detail: format!("duplicate op {op:?}"),
                });
            }
        }
        for (kind, versions) in &cap.speaks {
            if versions.is_empty() {
                issues.push(AssemblyIssue {
                    capability: cap.id.clone(),
                    check: "speaks".into(),
                    detail: format!("speaks of {kind:?} with no versions"),
                });
            }
        }
        for (kind, steps) in &cap.migrates {
            for step in steps {
                if step.from >= step.to {
                    issues.push(AssemblyIssue {
                        capability: cap.id.clone(),
                        check: "migrates".into(),
                        detail: format!("{kind}: migration {} -> {} does not move forward", step.from, step.to),
                    });
                }
            }
        }
    }

    // Dependency closure + producer coherence: every consumed
    // document kind is produced by a capability present in the
    // assembly, and producers actually speak what they produce.
    for cap in &manifest.capabilities {
        for (kind, producer_id) in &cap.consumes {
            let Some(producer) = by_id.get(producer_id.as_str()) else {
                issues.push(AssemblyIssue {
                    capability: cap.id.clone(),
                    check: "closure".into(),
                    detail: format!("consumes {kind:?} from {producer_id:?}, which is not in the assembly"),
                });
                continue;
            };
            if !producer.speaks.contains_key(kind) {
                issues.push(AssemblyIssue {
                    capability: cap.id.clone(),
                    check: "producer_speaks".into(),
                    detail: format!("{producer_id:?} produces {kind:?} but speaks no version of it"),
                });
            }
        }
    }

    // Version coherence, per document kind: every version any
    // capability speaks must be READABLE by every consumer of the
    // kind — spoken directly, or reachable through the migration
    // paths the assembly carries (any capability may provide the
    // bridge; the host routes the op). This is the check that makes
    // graceful degradation PREDICTABLE: a document can never be
    // unreadable-by-and-unmigratable-for a loaded consumer.
    let kinds: BTreeSet<&str> = manifest
        .capabilities
        .iter()
        .flat_map(|c| c.speaks.keys())
        .chain(manifest.capabilities.iter().flat_map(|c| c.consumes.keys()))
        .map(|k| k.as_str())
        .collect();
    for kind in kinds {
        let mut edges: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for cap in &manifest.capabilities {
            if let Some(steps) = cap.migrates.get(kind) {
                for step in steps {
                    edges.entry(step.from).or_default().insert(step.to);
                }
            }
        }
        // Transitive reachability (tiny graphs — versions per kind
        // are single digits).
        let reach = |from: u32| -> BTreeSet<u32> {
            let mut seen = BTreeSet::from([from]);
            let mut frontier = vec![from];
            while let Some(v) = frontier.pop() {
                if let Some(next) = edges.get(&v) {
                    for n in next {
                        if seen.insert(*n) {
                            frontier.push(*n);
                        }
                    }
                }
            }
            seen
        };
        for cap in &manifest.capabilities {
            if !cap.consumes.contains_key(kind) {
                continue; // readers of their own documents need no bridge
            }
            let Some(accepted) = cap.speaks.get(kind) else {
                issues.push(AssemblyIssue {
                    capability: cap.id.clone(),
                    check: "version_coherence".into(),
                    detail: format!("consumes {kind:?} but speaks no version of it"),
                });
                continue;
            };
            // Every version of the kind that exists in the assembly
            // (spoken by anyone) must be readable by this consumer.
            for other in &manifest.capabilities {
                let Some(existing) = other.speaks.get(kind) else { continue };
                for &version in existing {
                    if accepted.contains(&version) {
                        continue;
                    }
                    let reachable = reach(version);
                    if !accepted.iter().any(|a| reachable.contains(a)) {
                        issues.push(AssemblyIssue {
                            capability: cap.id.clone(),
                            check: "version_coherence".into(),
                            detail: format!(
                                "reads {kind:?} at {:?} but cannot read or migrate from v{version}",
                                accepted
                            ),
                        });
                    }
                }
            }
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}
