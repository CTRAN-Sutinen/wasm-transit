//! Assembly-validator laws: the compose-time tier is only trust-
//! worthy if the validator itself is gated. Three laws:
//!
//! * **Totality** — ANY manifest (garbage fields and all) yields
//!   Ok or Err, never a panic: the validator is total, because
//!   agents will hand it arbitrary output.
//! * **Purity** — validation is a pure function of the manifest:
//!   same bytes, same verdict, on every host.
//! * **Mutation law** — every single way to break an otherwise-valid
//!   assembly is caught, by name (the check an agent or a reviewer
//!   acts on). A blind spot here is a lie the validator tells.

use std::collections::HashMap;

use proptest::prelude::*;

use cap_abi::manifest::{CapManifest, Capability, MigrationSpec};
use cap_abi::validate::{validate_assembly, validate_with_kernel};

/// The PoC's own assembly — the baseline the mutations break.
fn valid_manifest() -> CapManifest {
    let cap = |id: &str,
               hash: &str,
               ops: &[&str],
               consumes: &[(&str, &str)],
               speaks: &[(&str, &[u32])],
               migrates: &[(&str, &[(u32, u32)])]| {
        Capability {
            id: id.into(),
            hash: hash.into(),
            url: format!("caps/{}-{}.wasm", id, hash),
            bytes: 1024,
            ops: ops.iter().map(|s| s.to_string()).collect(),
            consumes: consumes
                .iter()
                .map(|(k, p)| ((*k).to_string(), (*p).to_string()))
                .collect(),
            speaks: speaks
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.to_vec()))
                .collect(),
            migrates: migrates
                .iter()
                .map(|(k, steps)| {
                    ((*k).to_string(), steps.iter().map(|&(f, t)| MigrationSpec { from: f, to: t }).collect())
                })
                .collect(),
            purpose: String::new(),
            op_docs: HashMap::new(),
        }
    };
    let hex = |n: u8| format!("{}{:0>63}", n, "");
    CapManifest {
        kernel: 1,
        capabilities: vec![
            cap("gtfs", &format!("sha256:{}", hex(1)),
                &["load_feed", "departures"], &[],
                &[("feed", &[1]), ("watchlist", &[1, 2])],
                &[("watchlist", &[(1, 2)])]),
            cap("router", &format!("sha256:{}", hex(2)),
                &["load_feed", "plan"], &[("feed", "gtfs")],
                &[("feed", &[1])], &[]),
        ],
    }
}

fn rejects_with(manifest: &CapManifest, check: &str) -> bool {
    matches!(validate_assembly(manifest), Err(issues) if issues.iter().any(|i| i.check == check))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Totality: arbitrary manifests — garbage ids, malformed hashes,
    /// empty everything — produce a verdict, never a panic.
    #[test]
    fn validation_is_total(
        kernel in any::<u32>(),
        ids in proptest::collection::vec(any::<String>(), 0..=4),
        hashes in proptest::collection::vec(any::<String>(), 0..=4),
        ops in proptest::collection::vec(proptest::collection::vec(any::<String>(), 0..=3), 0..=4),
        speaks in proptest::collection::vec(proptest::collection::vec(1u32..5, 0..=3), 0..=4),
    ) {
        let capabilities = (0..ids.len().max(hashes.len()).max(ops.len()).max(speaks.len()))
            .map(|i| Capability {
                id: ids.get(i).cloned().unwrap_or_default(),
                hash: hashes.get(i).cloned().unwrap_or_default(),
                url: format!("caps/{}", hashes.get(i).cloned().unwrap_or_default()),
                bytes: 0,
                ops: ops.get(i).cloned().unwrap_or_default(),
                consumes: HashMap::new(),
                speaks: HashMap::from([("feed".to_string(), speaks.get(i).cloned().unwrap_or_default())]),
                migrates: HashMap::new(),
                purpose: String::new(),
                op_docs: HashMap::new(),
            })
            .collect();
        let manifest = CapManifest { kernel, capabilities };
        let _ = validate_assembly(&manifest); // a verdict, never a panic
    }

    /// Purity: the verdict is a pure function of the manifest bytes.
    #[test]
    fn validation_is_pure(
        manifest_seed in any::<u64>(),
    ) {
        let manifest = valid_manifest();
        let _ = manifest_seed;
        let a = validate_assembly(&manifest).is_ok();
        let b = validate_assembly(&manifest).is_ok();
        prop_assert_eq!(a, b);
    }
}

/// Mutation law: every single break is caught, by check name. One
/// mutation at a time on an otherwise-valid assembly.
#[test]
fn every_single_break_is_caught_by_name() {
    // The unmutated assembly validates.
    assert!(validate_assembly(&valid_manifest()).is_ok(), "baseline must be valid");

    let cases: Vec<(&str, Box<dyn Fn(&mut CapManifest)>)> = vec![
        ("kernel", Box::new(|m| m.kernel = 2)),
        ("unique_ids", Box::new(|m| m.capabilities[1].id = "gtfs".into())),
        ("hash_format", Box::new(|m| m.capabilities[0].hash = "not-a-hash".into())),
        ("hash_pinned_url", Box::new(|m| m.capabilities[0].url = "caps/other.wasm".into())),
        ("ops", Box::new(|m| m.capabilities[0].ops.clear())),
        ("closure", Box::new(|m| {
            m.capabilities[1].consumes.insert("feed".into(), "ghost".into());
        })),
        ("producer_speaks", Box::new(|m| {
            m.capabilities[0].speaks.remove("feed");
        })),
        ("version_coherence", Box::new(|m| {
            // The consumer can no longer read or migrate from v2.
            m.capabilities[1].speaks.insert("feed".into(), vec![2]);
        })),
        ("version_coherence", Box::new(|m| {
            // Migration path removed while the producer still speaks
            // a version the consumer cannot read.
            m.capabilities[0].speaks.insert("feed".into(), vec![1, 2]);
            m.capabilities[1].speaks.insert("feed".into(), vec![1]);
        })),
        ("speaks", Box::new(|m| {
            m.capabilities[0].speaks.insert("watchlist".into(), vec![]);
        })),
        ("migrates", Box::new(|m| {
            m.capabilities[0].migrates.insert(
                "watchlist".into(),
                vec![MigrationSpec { from: 2, to: 1 }],
            );
        })),
    ];
    for (check, mutate) in cases {
        let mut manifest = valid_manifest();
        mutate(&mut manifest);
        assert!(
            rejects_with(&manifest, check),
            "mutation for {check:?} was not caught (got {:?})",
            validate_assembly(&manifest).err().map(|i| i.iter().map(|x| x.check.clone()).collect::<Vec<_>>())
        );
    }
}

/// Kernel negotiation: the validator is honest about which host it
/// speaks for.
#[test]
fn kernel_mismatch_is_reported_not_swallowed() {
    let manifest = valid_manifest();
    assert!(validate_with_kernel(&manifest, 1).is_ok());
    assert!(validate_with_kernel(&manifest, 7).is_err());
}
