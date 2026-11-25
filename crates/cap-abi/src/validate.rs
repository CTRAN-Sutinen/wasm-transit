//! Each check reads only the manifest and holds for every future request,
//! as a type check does. A validated assembly cannot later get a document
//! that a loaded capability can neither speak nor migrate.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::manifest::{CapManifest, Capability};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AssemblyIssue {
    pub capability: String,
    pub check: String,
    pub detail: String,
}

/// The validator rejects a manifest that was built for a different kernel.
pub const HOST_KERNEL: u32 = 1;

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
        // The url must carry the hash. Otherwise a swap at the url would
        // defeat the pin.
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

    let mut uis = BTreeSet::new();
    for asm in &manifest.assemblies {
        let name = format!("(assembly {})", asm.ui);
        if !uis.insert(asm.ui.as_str()) {
            issues.push(AssemblyIssue { capability: name.clone(), check: "unique_ids".into(), detail: "duplicate assembly".into() });
        }
        let hex = asm.hash.strip_prefix("sha256:").unwrap_or("");
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) || !asm.url.contains(hex) {
            issues.push(AssemblyIssue {
                capability: name.clone(),
                check: "hash_pinned_url".into(),
                detail: format!("hash {:?} / url {:?} is not a pinned sha256", asm.hash, asm.url),
            });
        }
        if asm.capabilities.is_empty() {
            issues.push(AssemblyIssue { capability: name.clone(), check: "linked".into(), detail: "links no capability".into() });
        }
        let mut linked = BTreeSet::new();
        for id in &asm.capabilities {
            if !linked.insert(id.as_str()) {
                issues.push(AssemblyIssue { capability: name.clone(), check: "linked".into(), detail: format!("links {id:?} twice") });
            }
            let Some(cap) = by_id.get(id.as_str()) else {
                issues.push(AssemblyIssue {
                    capability: name.clone(),
                    check: "linked".into(),
                    detail: format!("links {id:?}, which the manifest does not carry"),
                });
                continue;
            };
            for (kind, producer) in &cap.consumes {
                if !asm.capabilities.contains(producer) {
                    issues.push(AssemblyIssue {
                        capability: name.clone(),
                        check: "closure".into(),
                        detail: format!("{id:?} consumes {kind:?} from {producer:?}, which is not linked"),
                    });
                }
            }
        }
    }

    // Every consumer of a kind must read every version that any capability
    // speaks, directly or through migration paths. Any capability may
    // provide the bridge, and the host routes the op.
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
        // A kind has single-digit versions, so the graphs are tiny.
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
                continue; // A reader of its own documents needs no bridge.
            }
            let Some(accepted) = cap.speaks.get(kind) else {
                issues.push(AssemblyIssue {
                    capability: cap.id.clone(),
                    check: "version_coherence".into(),
                    detail: format!("consumes {kind:?} but speaks no version of it"),
                });
                continue;
            };
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
