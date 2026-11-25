//! The generator writes the capability manifest after the wasm builds
//! (`just caps`) and publishes the registry (`just registry`). It takes the
//! capabilities as data ([`Linked`]) and names none. Its binary, gen-manifest,
//! is generated, and calls [`main`].
//!
//! It writes two outputs. `web/public/caps/` holds the browser's manifest and
//! the module files, named by content hash. `registry/` holds immutable blobs,
//! one evidence record per capability, a registry pointer, and the documents:
//! every kind's JSON Schema and a worked example (docs/FORMATS.md). An agent
//! verifies a module's bytes against its record.
//!
//! The generator checks the manifest with `cap_abi::validate` before it
//! writes it, so it never writes a manifest that the validator refuses.

use std::fs;
use std::path::Path;
use std::process::Command;

use cap_abi::manifest::{CapManifest, Capability, KernelOp, LinkedAssembly, MigrationSpec, OpDoc, OpExample};

// gen-assemblies stays in the kernel's crate, because it writes the module
// crates before any build.
#[path = "../../cap-abi/src/bin/common/assemblies.rs"]
mod common;
mod schema_lock;
pub mod kind_table;
pub use common::server_capabilities;
pub mod checks;
pub mod doc_check;
use fixtures::*;
use cap_abi::validate::validate_assembly;
use sha2::{Digest as _, Sha256};

/// The gates that `just registry` runs before publishing. It sets
/// `CAPS_PUBLISH_GATED=1`. A plain `just caps` publishes ungated, and the
/// record says so. The recipe is the authority.
const PUBLISH_GATES: &[&str] = &[
    "cargo test (workspace, includes the capability law suites)",
    "node crates/cap-abi/caps-smoke.mjs (ABI + composition + context)",
    "just reproducible (two clean builds of the tree, byte for byte)",
];

const KERNEL: u32 = 1;

pub struct Linked {
    pub declared: cap_abi::kernel::Declared,
    pub examples: fn() -> Vec<cap_abi::manifest::Example>,
}

fn kernel_ops(net: Option<&NetworkExampleContext>) -> Vec<KernelOp> {
    use cap_abi::kernel::{self, kind, loader, Cap};
    use cap_abi::registry::{OP_EVICT, OP_HELD};
    static EXAMPLE: Cap = Cap {
        id: "example",
        json: |_| String::new(),
        bytes: |_, _| String::new(),
        documents: &[loader::<transit_network::Network>, loader::<transit_rt::RtOverview>],
    };
    let answer = |text: String| serde_json::from_str::<serde_json::Value>(&text).expect("the kernel answers JSON");
    let rt_bytes = fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/sample-rt.pb")).expect("rt fixture");
    if let Some(ctx) = net {
        kind::<transit_network::Network>().register(ctx.network.clone()).expect("the fixture network registers");
    }
    let (rt_hash, _) = kind::<transit_rt::RtOverview>().register_bytes(&rt_bytes).expect("the rt fixture decodes");
    let held = answer(kernel::invoke_bytes(&[&EXAMPLE], OP_HELD, b""));
    let keep = serde_json::json!({ "keep": [rt_hash] });
    let evicted = answer(kernel::invoke_bytes(&[&EXAMPLE], OP_EVICT, keep.to_string().as_bytes()));
    let about = answer(kernel::invoke_bytes(&[&EXAMPLE], kernel::OP_ABOUT, b""));

    // A load's id, op, and code are its type's (D-239), and its line in
    // the kind list must agree.
    fn load<T: cap_abi::kernel::Load>(description: &str, net: Option<&NetworkExampleContext>) -> KernelOp {
        let line = cap_abi::kinds::by_load(T::ID)
            .unwrap_or_else(|| panic!("the kernel load {} has no line in crates/cap-abi/src/kinds.rs", T::LOAD_OP));
        let listed = line.load.expect("by_load finds a load");
        assert_eq!((listed.op, listed.code), (T::LOAD_OP, T::CODE), "{}: the kind list's op and code are its type's", T::ID);
        let ty = std::any::type_name::<T>();
        let (krate, name) = (ty.split("::").next().unwrap(), ty.rsplit("::").next().unwrap());
        assert!(
            line.library.starts_with(&format!("{krate}::")) && line.library.ends_with(&format!("::{name}")),
            "{}: the line names {}, and the load is {ty}'s",
            T::ID,
            line.library
        );
        KernelOp {
            code: T::CODE,
            name: T::LOAD_OP.into(),
            kind: Some(T::ID.into()),
            description: description.into(),
            example: kernel_example(T::LOAD_OP, net),
        }
    }
    let ops = vec![
        load::<transit_network::Network>("Register the NETWORK document under its content hash (idempotent) — one identity in every module that reads networks, registered once per instance however many of its capabilities read it. JSON: {\"op\": \"load_network\", \"network\": …}. Bulk: the bare document's JSON bytes, or those bytes gzipped (decoded as they parse — the text never exists whole in the module) — the same answer and hash, no request envelope; hosts send real-size networks this way, gzipped.", net),
        load::<transit_sync::EditDocument>("Register the OPERATOR EDIT document ({\"kind\": \"edit_records\", \"schema_version\", \"records\"} — wholesale-replaced by the host: push the whole document on every change) under its canonical hash, which the day views address. Only the version this module reads registers: an older document is refused naming the migration capability, a newer one naming both versions. A record the network cannot answer simply patches nothing — what each record amounts to on a network is the divert capability's reports. JSON: {\"op\": \"load_edits\", \"edits\": {…}}; bulk: the document's JSON.", net),
        load::<transit_live::LiveDay>("Register a LIVE document (the rt capability's live answer) under its canonical hash — the same identity rt answered (in a module linking rt, its live answer is registered already). Ephemeral: the last 4 stay. JSON: {\"op\": \"load_live\", \"live\": …}; bulk: its JSON.", net),
        load::<transit_codec::Boundary>("Register an imported network's BOUNDARY (the network capability's import answer) under its canonical hash: the stop_sequence numbering the published feed — and so its rt updates — use. A network published from its own export needs none (1..n). JSON: {\"op\": \"load_boundary\", \"boundary\": …}; bulk: its JSON.", net),
        load::<transit_streets::walking::WalkTable>("Register a WALK TABLE (the streets capability's walks answer: every walk between the network's stops along the streets) under its canonical hash — the same identity streets answered. A plan naming it by walks_hash walks its transfers along the streets; a table derived for another network is refused there. JSON: {\"op\": \"load_walks\", \"walks\": …}; bulk: its JSON.", net),
        load::<transit_streets::Streets>("Register a STREETS document the host holds (an import's answer, stored) under its hash — the same hash its import answered — with its size (ways, segments, distinct points). Refused unless canonical (ways of two or more points, sorted, none twice, two-ways in canonical direction). JSON: {\"op\": \"load_streets\", \"streets\": …}; bulk: the bare document's JSON bytes.", net),
        load::<transit_rider::watchlist::Watchlist>("Register a WATCHLIST document (the personal saved set, version-tagged) under its canonical hash, answering its version. JSON: {\"op\": \"load_watchlist\", \"document\": …}; bulk: its JSON.", net),
        load::<transit_analysis::WeightedLayer>("Register a GeoJSON WEIGHTED LAYER — Point, Polygon and MultiPolygon features (people, jobs, households: any weighted geography; other geometries are skipped) — under the content hash of its bytes. The answer is the import handle — features, points, polygons, and the weights it can be read by (each property that is a JSON number, with its total) — so the layer's owner never parses a second copy. Which field is the weight, and what it means, are the coverage op's (`field`, `label`): one file read two ways is one layer. Bulk: the GeoJSON bytes; JSON: {\"op\": \"load_layer\", \"bytes\": [..]}.", net),
        load::<transit_codec::Feed>("Register a GTFS zip under the content hash of its bytes (idempotent), answering the feed's summary — stops, routes, trips, stop_times — the at-a-glance read the feed line shows. Bulk: the raw zip.", net),
        load::<transit_runs::WorkRules>("Register the WORK RULES (transit-runs: the rules work is cut under — recovery per route and/or service type in minutes and/or a percent of the run, the route pairs one vehicle may interline, the run rules, pay; every rule optional, D-50) under its canonical hash, answering its version. Checked and normalized at the load (rules and pairs sorted, each pair's routes in order — two orderings are one identity); refused: another version, a rule requiring nothing, one scope twice, a share over 1000 percent, a pair of one route or listed twice, a minimum piece over the maximum. JSON: {\"op\": \"load_work_rules\", \"work_rules\": …}; bulk: its JSON.", net),
        load::<transit_stops::Stops>("Register a STOPS document (transit-stops: every stop a list of dated events — planned, permitted, built, in_service, suspended, moved, changed, retired; D-153, D-157) under its canonical hash — the same hash the stops capability's validate answers — with how many stops it holds. Refused unless validate would pass it, every refusal named. JSON: {\"op\": \"load_stops\", \"stops\": …}; bulk: its JSON.", net),
        load::<transit_requests::StopRequests>("Register a STOP REQUESTS document (transit-requests: the planning team's requests for work at a stop — new, reinstall, relocate/remove, repair/improve, temporary — each tracking the work order the work order system opens for it; D-159) under its canonical hash — the same hash the requests capability's validate answers — with how many requests it holds. Refused unless validate would pass it, every refusal named. JSON: {\"op\": \"load_stop_requests\", \"requests\": …}; bulk: its JSON.", net),
        load::<transit_alerts::ServiceAlerts>("Register a SERVICE ALERTS document (transit-alerts: the agency's alerts in the network's ids, each with its cause, effect, severity, texts in each language, periods, and informed entities; D-206) under its canonical hash, the same hash the alerts capability's validate answers, with how many alerts it holds. Refused unless validate would pass it, every refusal named. JSON: {\"op\": \"load_service_alerts\", \"alerts\": …}; bulk: its JSON.", net),
        load::<transit_alerts::AlertTemplates>("Register an ALERT TEMPLATES document (transit-alerts: the agency's standard language for alerts, with blanks; D-210) under its canonical hash, the same hash the alerts capability's validate_templates answers, with how many templates it holds. Refused unless validate_templates would pass it, every refusal named. JSON: {\"op\": \"load_alert_templates\", \"templates\": …}; bulk: its JSON.", net),
        load::<transit_notes::Notes>("Register a NOTES document (transit-notes: the agency's internal notes, each about a trip instance, an edit record, a service alert, or a stop, with who wrote it and when; D-214) under its canonical hash, the same hash the notes capability's validate answers, with how many notes it holds. Refused unless validate would pass it, every refusal named. JSON: {\"op\": \"load_notes\", \"notes\": …}; bulk: its JSON.", net),
        load::<transit_rt::RtOverview>("Register a GTFS-rt protobuf payload (a snapshot) under the content hash of its bytes, answering the hash only (the rt capability's decode answers the overview too). Ephemeral: the last 8 stay. Bulk: the raw payload.", net),
        KernelOp {
            code: OP_EVICT,
            name: "evict".into(),
            kind: None,
            description: "EVERY module. Drop everything the keep list does not name, answering what dropped and what is held after. The bytes are {\"keep\": [hash, …]}: a document stays when its hash is kept; a derived entry (a cached day, the router's graph, a street graph) stays only while every document it was computed from is kept. The host names what survives, because it owns the documents. The module drops the rest. Asking for an evicted document is the unknown-hash error (`unknown <kind> <hash>: <load op> first`), never a wrong answer: register it again. Ephemeral kinds also bound themselves with no host steering — rt snapshots keep their last 8, live documents their last 4 — and so does each derivation (held days 4, router graphs 8, street and walking graphs 4).".into(),
            example: Some(OpExample { request: keep, response: evicted }),
        },
        KernelOp {
            code: OP_HELD,
            name: "held".into(),
            kind: None,
            description: "EVERY module. What the module holds, per registry (each document kind its capabilities read, then the derived entries), oldest first (the bytes are ignored) — the inspection a memory gate reads.".into(),
            example: Some(OpExample { request: serde_json::json!(null), response: held }),
        },
        KernelOp {
            code: kernel::OP_ABOUT,
            name: "about".into(),
            kind: None,
            description: "EVERY module. What the module is: the capabilities it links in index order (an assembly's capability bulk op carries its index in the high 16 bits; a JSON request names its capability as \"cap\" when the module links more than one), each with the document kinds it reads (the bytes are ignored).".into(),
            example: Some(OpExample { request: serde_json::json!(null), response: about }),
        },
    ];
    let listed: std::collections::BTreeSet<&str> = cap_abi::kinds::KINDS.iter().filter_map(|k| k.load.map(|l| l.id)).collect();
    let loaded: std::collections::BTreeSet<&str> = ops.iter().filter_map(|op| op.kind.as_deref()).collect();
    assert_eq!(listed, loaded, "each line of crates/cap-abi/src/kinds.rs with a load is a kernel load");
    ops
}

/// The generator refuses an OPS.md whose heading is not the capability's id,
/// or whose ops are not exactly its `Request` enum's.
pub struct CapDocs {
    pub purpose: String,
    pub ops: Vec<(String, String)>,
}

pub fn read_docs(module: &cap_abi::kernel::Declared) -> CapDocs {
    let text = module.docs;
    let place = format!("crates/cap-{}/OPS.md", module.id);
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            sections.push((h.trim().to_string(), Vec::new()));
        } else if let Some(h) = line.strip_prefix("# ") {
            assert!(sections.is_empty(), "{place}: one `#` heading, first");
            assert_eq!(h.trim(), module.id, "{place}: the `#` heading names the capability");
            sections.push((String::new(), Vec::new()));
        } else {
            sections.last_mut().unwrap_or_else(|| panic!("{place}: starts with `# {}`", module.id)).1.push(line);
        }
    }
    let body = |lines: &[&str]| lines.join("\n").trim_matches('\n').to_string();
    let mut it = sections.into_iter();
    let (head, purpose) = it.next().unwrap_or_else(|| panic!("{place}: empty"));
    assert!(head.is_empty(), "{place}: starts with `# {}`", module.id);
    let ops: Vec<(String, String)> = it.map(|(op, lines)| (op, body(&lines))).collect();
    let names: Vec<&str> = ops.iter().map(|(op, _)| op.as_str()).collect();
    // The enum says which ops exist. The file says their order.
    let mut sorted = names.clone();
    sorted.sort_unstable();
    let mut ops_sorted: Vec<&str> = module.ops.iter().map(String::as_str).collect();
    ops_sorted.sort_unstable();
    assert_eq!(sorted, ops_sorted, "{place}: one `## <op>` section for each op of its `Request` enum, and no other");
    for (op, d) in &ops {
        assert!(!d.is_empty(), "{place}: `## {op}` says nothing");
    }
    CapDocs { purpose: body(&purpose), ops }
}

pub fn main(capabilities: &[Linked]) {
    let caps = capabilities;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let profile = std::env::var("CAPS_PROFILE").unwrap_or_else(|_| "release".into());
    let target = root.join("target/wasm32-unknown-unknown").join(&profile);
    let dist = root.join("web/public/caps");

    let fixture = root.join("web/e2e/fixtures/sample-gtfs.zip");
    let feed_bytes = fixtures::feed_bytes();

    // Computed through transit-codec, the library that the module uses, so
    // the examples cannot drift from what the module returns.
    let net_ctx = network_example_context(&feed_bytes);
    // The kernel's examples run first, because its registry ops answer what
    // the process holds.
    let kernel_ops = kernel_ops(net_ctx.as_ref());

    // `--lock` (`just schema-lock`) records what this build adds and stops.
    // A plain run fails on any drift. Both run before anything is written.
    let documents = document_kinds(net_ctx.as_ref(), caps);
    let locking = std::env::args().any(|a| a == "--lock");
    let locks = lock_documents(root, &documents, locking);
    let (table, table_schemas) = kind_table(root, &documents, &locks);
    lock_kind_table(root, &table, locking);
    if locking {
        return;
    }

    // The filename carries the content hash, so the HTTP cache holds each
    // build forever.
    let _ = fs::remove_dir_all(&dist);
    fs::create_dir_all(&dist).expect("create caps dir");
    fs::copy(&fixture, dist.join("sample-gtfs.zip")).expect("copy demo feed");

    // The runner gates chain from this load_network request, which uses the
    // same bytes as the examples and the smoke.
    if let Some(ctx) = net_ctx.as_ref() {
        let request = serde_json::json!({ "op": "load_network", "network": ctx.derived });
        let json = serde_json::to_string_pretty(&request).expect("request serializes");
        fs::write(dist.join("sample-network-load.json"), json + "\n")
            .expect("write the dispatch sample document");
    }

    let mut capabilities = Vec::with_capacity(caps.len());
    for linked in caps {
        let module = &linked.declared;
        let docs = read_docs(module);
        let file = format!("module_{}.wasm", module.id);
        let bytes = fs::read(target.join(&file)).expect("wasm module (run the build first)");
        let hex = format!("{:x}", Sha256::digest(&bytes));
        fs::write(dist.join(format!("{}-{hex}.wasm", module.id)), &bytes).expect("write module");
        capabilities.push(Capability {
            id: module.id.into(),
            hash: format!("sha256:{hex}"),
            url: format!("caps/{}-{hex}.wasm", module.id),
            bytes: bytes.len() as u64,
            ops: docs.ops.iter().map(|(op, _)| op.clone()).collect(),
            documents: module.documents.iter().map(|s| s.to_string()).collect(),
            consumes: module
                .consumes
                .iter()
                .map(|(kind, id)| ((*kind).to_string(), (*id).to_string()))
                .collect(),
            speaks: module
                .speaks
                .iter()
                .map(|(kind, versions)| {
                    ((*kind).to_string(), versions.iter().copied().collect::<Vec<_>>())
                })
                .collect(),
            migrates: module
                .migrates
                .iter()
                .map(|(kind, steps)| {
                    (
                        (*kind).to_string(),
                        steps
                            .iter()
                            .map(|&(from, to)| MigrationSpec { from, to })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect(),
            purpose: docs.purpose.clone(),
            op_docs: docs
                .ops
                .iter()
                .map(|(op, description)| {
                    let (example, kinds) = match run_examples(caps, module.id, op) {
                        Some((example, kinds)) => (Some(example), kinds),
                        None => (None, Vec::new()),
                    };
                    (
                        op.clone(),
                        OpDoc { description: description.clone(), example, kinds },
                    )
                })
                .collect(),
        });
        println!("cap {}: sha256:{hex} ({:.1} KiB)", module.id, bytes.len() as f64 / 1024.0);
    }

    let mut assemblies = Vec::new();
    for assembly in common::assemblies(root).into_iter().filter(|a| a.ui.is_some()) {
        let ui = assembly.ui.clone().expect("a UI assembly");
        let file = format!("{}.wasm", assembly.name.replace('-', "_"));
        let bytes = fs::read(target.join(&file)).expect("assembly module (run the build first)");
        let hex = format!("{:x}", Sha256::digest(&bytes));
        fs::write(dist.join(format!("ui-{ui}-{hex}.wasm")), &bytes).expect("write assembly");
        println!("assembly {ui}: sha256:{hex} ({:.1} KiB: {})", bytes.len() as f64 / 1024.0, assembly.caps.join(", "));
        assemblies.push(LinkedAssembly {
            ui: ui.clone(),
            hash: format!("sha256:{hex}"),
            url: format!("caps/ui-{ui}-{hex}.wasm"),
            bytes: bytes.len() as u64,
            capabilities: assembly.caps,
        });
    }

    let manifest = CapManifest { kernel: KERNEL, kernel_ops, capabilities, assemblies };

    if let Err(issues) = validate_assembly(&manifest) {
        eprintln!("generated assembly is NOT valid:");
        for issue in issues {
            eprintln!("  [{}] {}: {}", issue.capability, issue.check, issue.detail);
        }
        std::process::exit(1);
    }

    let json = serde_json::to_string_pretty(&manifest).expect("manifest serializes");
    fs::write(dist.join("manifest.json"), json + "\n").expect("write manifest");

    // The local registry follows every build, fast ones too, so caps-run
    // runs the code that was just changed. The evidence names the profile.
    // Only `just registry` (gated, release) is a publish.
    publish_registry(root, &manifest, &documents, &locks, &profile);
    publish_kind_table(root, &table, &table_schemas);

    println!("manifest: {} capabilities, kernel v{KERNEL}, assembly valid", caps.len());
}

fn toolchain_pin(root: &Path) -> String {
    let text = fs::read_to_string(root.join("rust-toolchain.toml")).expect("rust-toolchain.toml (the toolchain pin)");
    text.lines()
        .find_map(|l| l.trim().strip_prefix("channel").map(|v| v.trim().trim_start_matches('=').trim().trim_matches('"').to_string()))
        .expect("rust-toolchain.toml names a channel")
}

/// The evidence record says which gates the publish recipe ran against the
/// bytes, and at which commit, so "known good" is a checkable claim.
fn publish_registry(root: &Path, manifest: &CapManifest, documents: &[DocumentKind], locks: &[schema_lock::Lock], profile: &str) {
    let registry = root.join("registry");
    let blobs = registry.join("blobs");
    let caps_dir = registry.join("capabilities");
    let _ = fs::remove_dir_all(&registry);
    fs::create_dir_all(&blobs).expect("create registry blobs");
    fs::create_dir_all(&caps_dir).expect("create registry records");

    let run = |args: &[&str]| {
        Command::new(args[0])
            .args(&args[1..])
            .current_dir(root)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    let commit = run(&["git", "rev-parse", "HEAD"]);
    let dirty = !run(&["git", "status", "--porcelain"]).is_empty();
    let rustc = run(&["rustc", "--version"]);
    let gated = std::env::var("CAPS_PUBLISH_GATED").is_ok_and(|v| v == "1");
    assert!(!gated || profile == "release", "a gated publish takes release modules, not {profile} (unset CAPS_PROFILE)");
    // A gated publish refuses a compiler other than the pin, so the
    // published hashes are rebuildable from the tree alone.
    let verbose = run(&["rustc", "-vV"]);
    let field = |name: &str| {
        verbose
            .lines()
            .find_map(|l| l.strip_prefix(name).map(|v| v.trim().to_string()))
            .unwrap_or_default()
    };
    let pinned = toolchain_pin(root);
    let release = field("release:");
    if gated {
        assert_eq!(
            release, pinned,
            "rustc {release} is not the pinned toolchain {pinned} (rust-toolchain.toml)"
        );
    }
    let toolchain = serde_json::json!({
        "pinned": pinned,
        "rustc": release,
        "rustc_commit": field("commit-hash:"),
        "llvm": field("LLVM version:"),
        "cargo": run(&["cargo", "--version"]),
        "locked": true,
        "remapped_paths": ["/cargo", "/src"],
    });
    let evidence = serde_json::json!({
        "gates": if gated { PUBLISH_GATES } else { &[] },
        "recipe": if gated { "just registry" } else if profile == "release" { "just caps" } else { "just caps-fast" },
        "profile": profile,
        "source_commit": commit,
        "dirty_tree": dirty,
        "source_digest": source_digest(root),
        "rustc": rustc,
        "toolchain": toolchain,
    });

    for cap in &manifest.capabilities {
        let bytes = fs::read(root.join("web/public/caps").join(Path::new(&cap.url).file_name().unwrap()))
            .expect("module bytes (the manifest step wrote them)");
        let hex = cap.hash.strip_prefix("sha256:").expect("hash format");
        fs::write(blobs.join(format!("{hex}.wasm")), &bytes).expect("write blob");

        let record = serde_json::json!({
            "id": cap.id,
            "hash": cap.hash,
            "bytes": cap.bytes,
            "kernel": manifest.kernel,
            "ops": cap.ops,
            "documents": cap.documents,
            "consumes": cap.consumes,
            "speaks": cap.speaks,
            "migrates": cap.migrates,
            "purpose": cap.purpose,
            "op_docs": cap.op_docs,
            "blob": format!("blobs/{hex}.wasm"),
            "evidence": evidence,
        });
        let json = serde_json::to_string_pretty(&record).expect("record serializes");
        fs::write(caps_dir.join(format!("{}.json", cap.id)), json + "\n")
            .expect("write record");
    }

    // Blobs are immutable. This file moves.
    let pointer = serde_json::json!({
        "kernel": manifest.kernel,
        // `caps-run --load <kind>:FILE` resolves its kind from these.
        "kernel_ops": manifest.kernel_ops,
        "capabilities": manifest
            .capabilities
            .iter()
            .map(|c| serde_json::json!({ "id": c.id, "hash": c.hash }))
            .collect::<Vec<_>>(),
        "evidence": evidence,
    });
    let json = serde_json::to_string_pretty(&pointer).expect("pointer serializes");
    fs::write(registry.join("registry.json"), json + "\n").expect("write pointer");

    publish_documents(&registry, documents, locks, &evidence);

    println!(
        "registry: {} capabilities published (blobs + records + pointer)",
        manifest.capabilities.len()
    );
}

struct DocumentKind {
    line: &'static cap_abi::kinds::Declaration,
    spec: schema_lock::Kind,
}

impl DocumentKind {
    fn store_kind(&self) -> Option<&'static str> {
        self.line.store.map(|s| s.kind)
    }
}

/// A type whose tag has no line in the kind list stops the run.
fn kind<T>(example: Option<serde_json::Value>, migrate: Option<schema_lock::Migrate>) -> DocumentKind
where
    T: transit_version::Versioned + schemars::JsonSchema + serde::Serialize + serde::de::DeserializeOwned,
{
    let line = cap_abi::kinds::by_tag(T::KIND)
        .unwrap_or_else(|| panic!("the tag {:?} has no line in crates/cap-abi/src/kinds.rs: a kind exists only if it has one", T::KIND));
    DocumentKind { line, spec: schema_lock::kind::<T>(T::KIND, T::READS, example, migrate) }
}

fn lock_documents(root: &Path, documents: &[DocumentKind], locking: bool) -> Vec<schema_lock::Lock> {
    let mut drifts = Vec::new();
    let mut locks = Vec::new();
    for doc in documents {
        let owner = doc.line.owner;
        let dir = root.join("crates").join(owner).join("locks");
        let lock = schema_lock::read_lock(&dir, doc.spec.tag).unwrap_or_else(|e| panic!("{owner}'s lock: {e}"));
        if !locking {
            drifts.extend(schema_lock::check(&doc.spec, &lock));
            locks.push(lock);
            continue;
        }
        match schema_lock::freeze(&doc.spec, &lock) {
            Ok(next) if next == lock => locks.push(lock),
            Ok(next) => {
                schema_lock::write_lock(&dir, &next).unwrap_or_else(|e| panic!("{owner}'s lock: {e}"));
                println!("schema lock: crates/{owner}/locks/ records {} v{}", doc.spec.tag, doc.spec.current);
                locks.push(next);
            }
            Err(refused) => {
                drifts.extend(refused);
                locks.push(lock);
            }
        }
    }
    if !drifts.is_empty() {
        eprintln!("schema lock: the code differs from what the documents promise (docs/FORMATS.md, \"The schema lock\"):");
        for drift in &drifts {
            eprintln!("  - {drift}");
        }
        std::process::exit(1);
    }
    locks
}

fn migrate_network(doc: serde_json::Value, _from: u32) -> Result<serde_json::Value, String> {
    transit_network::migrate_to(doc, transit_network::SCHEMA_VERSION).map(|(doc, _)| doc)
}

fn migrate_watchlist(doc: serde_json::Value, _from: u32) -> Result<serde_json::Value, String> {
    transit_rider::watchlist::migrate_to(doc, transit_rider::watchlist::SCHEMA_VERSION).map(|(doc, _)| doc)
}

fn migrate_alerts(doc: serde_json::Value, _from: u32) -> Result<serde_json::Value, String> {
    transit_alerts::version::migrate_to(doc, transit_alerts::ALERTS_VERSION).map(|(doc, _)| doc)
}

fn migrate_requests(doc: serde_json::Value, _from: u32) -> Result<serde_json::Value, String> {
    transit_requests::version::migrate_to(doc, transit_requests::SCHEMA_VERSION)
}

fn migrate_edits(doc: serde_json::Value, _from: u32) -> Result<serde_json::Value, String> {
    transit_sync::version::migrate_to(doc, transit_sync::SCHEMA_VERSION).map(|(doc, _)| doc)
}

/// The feed (a GTFS zip) and the rt snapshot (protobuf) have their own
/// specs, and the manifest is the Go host's own document (docs/FORMATS.md).
fn document_kinds(net: Option<&NetworkExampleContext>, caps: &[Linked]) -> Vec<DocumentKind> {
    let plans_example = net.and_then(|n| {
        let set = fixture_plan_set().iter().fold(transit_plans::PublishedPlans::empty(), |set, plan| {
            transit_plans::publish(&set, plan, &n.network_hash, fixture_today()).unwrap_or(set)
        });
        serde_json::to_value(set).ok()
    });
    let walks_example = caps
        .iter()
        .find(|c| c.declared.ops.iter().any(|op| op == "walks"))
        .and_then(|c| run_examples(caps, c.declared.id, "walks")).and_then(|(ex, _)| ex.response.get("ok").and_then(|ok| ok.get("walks")).cloned());
    vec![
        kind::<transit_network::Network>(net.map(|n| n.derived.clone()), Some(migrate_network)),
        kind::<transit_codec::Boundary>(net.map(|n| n.boundary.clone()), None),
        kind::<transit_sync::EditDocument>(Some(value(&transit_sync::EditDocument::new(dispatch_fixture_edits()))),
            Some(migrate_edits),
        ),
        kind::<transit_log::Entry>(Some(value(&fixture_stops_log().1[1])), None),
        kind::<transit_log::Checkpoint>(Some(value(&fixture_stops_log().0)), None),
        kind::<transit_network::PlanSet>(net.map(|n| value(&transit_network::PlanSet { plans: [("plan-1".to_string(), n.derived_network().clone())].into(), ..Default::default() })),
            None,
        ),
        kind::<transit_rider::watchlist::Watchlist>(Some(value(&fixture_watchlist())),
            Some(migrate_watchlist),
        ),
        kind::<transit_plans::PublishedPlans>(plans_example, None),
        kind::<transit_plans::FrozenVersions>(net.and_then(|n| {
                let frozen = transit_plans::freeze(&transit_plans::FrozenVersions::empty(), &fixture_plan_set()[0], &n.network_hash, "proposal for public comment").ok()?;
                serde_json::to_value(frozen).ok()
            }),
            None,
        ),
        kind::<transit_rider::device::RecentPlaces>(Some(serde_json::json!({"kind": "recent_places", "schema_version": 1, "places": [
                {"id": "A", "name": "Main St & 1st Ave", "lat": 45.5, "lon": -73.5},
                {"id": "STN", "name": "Central Station"}
            ]})),
            None,
        ),
        kind::<transit_rider::device::LiveFeeds>(Some(serde_json::json!({"kind": "live_feeds", "schema_version": 1,
                "trips": "https://agency.example/gtfs-rt/trip-updates.pb", "vehicles": "", "alerts": ""})),
            None,
        ),
        kind::<transit_authz::Policy>(Some(value(&transit_authz::Policy::current(vec![transit_authz::Rule {
                name: "ops-write-edits".into(),
                roles: vec!["dispatcher".into()],
                pointers: vec!["edits".into()],
                actions: vec!["write".into()],
                extra: Default::default(),
            }]))),
            None,
        ),
        kind::<transit_streets::Streets>(streets_example_context().map(|c| value(&c.streets)), None),
        kind::<transit_streets::walking::WalkTable>(walks_example, None),
        kind::<transit_runs::WorkRules>(Some(value(&fixture_work_rules())), None),
        // The example holds no versions, because they would move it with
        // every schema.
        kind::<kind_table::KindTable>(
            Some(value(&kind_table::KindTable {
                kind: kind_table::KindTableKind,
                schema_version: kind_table::SCHEMA_VERSION,
                kinds: kind_table::kinds(),
                extra: Default::default(),
            })),
            None,
        ),
        kind::<transit_stops::Stops>(Some(value(&fixture_stops())), None),
        kind::<transit_requests::StopRequests>(Some(value(&fixture_requests())), Some(migrate_requests)),
        kind::<transit_alerts::ServiceAlerts>(Some(value(&fixture_alerts())), Some(migrate_alerts)),
        kind::<transit_alerts::AlertTemplates>(Some(value(&fixture_templates())), None),
        kind::<transit_notes::Notes>(Some(value(&fixture_notes())), None),
        kind::<transit_live::LiveDay>(live_example().map(|(live, _)| value(&live)), None),
    ]
}

fn value<T: serde::Serialize>(example: &T) -> serde_json::Value {
    serde_json::to_value(example).expect("example serializes")
}

/// The files ride in every page build (`web/public/schemas/`), so a rollback
/// serves the schemas of the build it moves to. The server copies the
/// `Link` header values from `links` and decides nothing.
fn publish_web_schemas(registry: &Path, index: &[serde_json::Value]) {
    let web = registry.parent().expect("the registry is in the tree").join("web/public/schemas");
    let _ = fs::remove_dir_all(&web);
    fs::create_dir_all(&web).expect("create web/public/schemas");
    let mut served = Vec::new();
    let (mut stored, mut published) = (serde_json::Map::new(), serde_json::Map::new());
    for entry in index {
        let mut entry = entry.clone();
        for (field, suffix) in [("schema", ".schema.json"), ("example", ".example.json")] {
            let Some(path) = entry[field].as_str().map(str::to_string) else { continue };
            let file = format!("{}.v{}{suffix}", entry["tag"].as_str().expect("an entry has its tag"), entry["schema_version"]);
            fs::copy(registry.join(&path), web.join(&file)).expect("copy a schema file");
            entry[field] = file.into();
        }
        let link = format!("</schemas/{}>; rel=\"describedby\"; type=\"application/schema+json\"", entry["schema"].as_str().expect("a schema"));
        if let Some(store_kind) = entry["store_kind"].as_str() {
            stored.insert(format!("{store_kind} v{}", entry["schema_version"]), link.clone().into());
        }
        if entry["written"] == true {
            published.insert(entry["tag"].as_str().expect("a tag").to_string(), link.into());
        }
        served.push(entry);
    }
    let links = serde_json::json!({ "stored": stored, "published": published });
    let json = serde_json::to_string_pretty(&serde_json::json!({ "documents": served, "links": links })).expect("the served index serializes");
    fs::write(web.join("documents.json"), json + "\n").expect("write the served index");
}

/// Kinds with a hand-written schema in `server/schemas/`.
const HOST_OWNED: &[(&str, u32)] = &[("build", 1), ("derivation", 1), ("store_layout", 1)];

/// Files are named by the tag that a document carries inside it, so a
/// reader holding only a document finds its schema. `written` says whether
/// this build writes that version or reads it only through a migration.
/// `go test ./server/...` validates every example against its schema with
/// no Rust.
fn publish_documents(registry: &Path, documents: &[DocumentKind], locks: &[schema_lock::Lock], evidence: &serde_json::Value) {
    let schemas = registry.join("schemas");
    let examples = registry.join("examples");
    fs::create_dir_all(&schemas).expect("create registry schemas");
    fs::create_dir_all(&examples).expect("create registry examples");
    let mut index = Vec::new();
    for (doc, lock) in documents.iter().zip(locks) {
        let tag = doc.spec.tag;
        for (&version, locked) in &lock.versions {
            let text = &locked.example;
            let stem = format!("{tag}.v{version}");
            let schema = published_schema(tag, version, &locked.schema);
            let schema_path = format!("schemas/{stem}.schema.json");
            fs::write(registry.join(&schema_path), &schema).expect("write schema");
            let mut entry = serde_json::json!({
                "kind": doc.line.name,
                "tag": tag,
                "store_kind": doc.store_kind(),
                "owner": doc.line.owner,
                "schema_version": version,
                "written": version == doc.spec.current,
                "schema": schema_path,
                "schema_hash": cap_abi::content_hash(schema.as_bytes()),
            });
            let example_path = format!("examples/{stem}.json");
            fs::write(registry.join(&example_path), text).expect("write example");
            entry["example"] = example_path.into();
            entry["example_hash"] = cap_abi::content_hash(text.as_bytes()).into();
            index.push(entry);
        }
    }
    // No Rust type generates these. `server/schema_test.go` holds them.
    // The manifest's schema is the HTTP contract's (`server/openapi.json`).
    for &(tag, version) in HOST_OWNED {
        let stem = format!("{tag}.v{version}");
        let dir = registry.parent().expect("the registry is in the tree").join("server/schemas");
        let read = |file: String| fs::read_to_string(dir.join(&file)).unwrap_or_else(|e| panic!("server/schemas/{file}: {e}"));
        let schema = published_schema(tag, version, &host_owned_schema(registry.parent().expect("the registry is in the tree"), tag, version));
        let example = read(format!("{stem}.example.json"));
        let (schema_path, example_path) = (format!("schemas/{stem}.schema.json"), format!("examples/{stem}.json"));
        fs::write(registry.join(&schema_path), &schema).expect("write schema");
        fs::write(registry.join(&example_path), &example).expect("write example");
        index.push(serde_json::json!({
            "kind": tag,
            "tag": tag,
            "store_kind": tag,
            "owner": "server",
            "schema_version": version,
            "written": true,
            "schema": schema_path,
            "schema_hash": cap_abi::content_hash(schema.as_bytes()),
            "example": example_path,
            "example_hash": cap_abi::content_hash(example.as_bytes()),
        }));
    }
    let json = serde_json::to_string_pretty(&serde_json::json!({ "documents": index, "evidence": evidence }))
        .expect("documents index serializes");
    fs::write(registry.join("documents.json"), json + "\n").expect("write documents index");
    publish_web_schemas(registry, &index);
    println!("registry: {} document versions published (schemas + worked examples)", index.len());
}

/// A function of the list and the locks alone, so the checked-in copy
/// (`crates/cap-abi/kinds.json`) needs no module build (D-232).
fn kind_table(root: &Path, documents: &[DocumentKind], locks: &[schema_lock::Lock]) -> (kind_table::KindTable, Vec<(String, String)>) {
    let mut kinds = kind_table::kinds();
    let mut schemas = Vec::new();
    let mut versions = |tag: &str, version: u32, schema: &serde_json::Value| -> Option<kind_table::VersionEntry> {
        let text = published_schema(tag, version, schema);
        let hash = cap_abi::content_hash(text.as_bytes());
        schemas.push((hash.clone(), text));
        Some(kind_table::VersionEntry { version, schema: hash })
    };
    for (doc, lock) in documents.iter().zip(locks) {
        let Some(entry) = kinds.iter_mut().find(|k| k.tag == doc.spec.tag) else { continue };
        for (&version, locked) in &lock.versions {
            entry.versions.extend(versions(doc.spec.tag, version, &locked.schema));
        }
        if let Some(current) = lock.versions.get(&doc.spec.current) {
            entry.references = transit_version::references(&current.schema);
        }
    }
    for &(tag, version) in HOST_OWNED {
        let entry = kinds.iter_mut().find(|k| k.tag == tag).unwrap_or_else(|| panic!("the host-owned {tag} has no line in the kind list"));
        let schema = host_owned_schema(root, tag, version);
        entry.versions.extend(versions(tag, version, &schema));
    }
    let table = kind_table::KindTable { kind: kind_table::KindTableKind, schema_version: kind_table::SCHEMA_VERSION, kinds, extra: Default::default() };
    (table, schemas)
}

const KIND_TABLE: &str = "crates/cap-abi/kinds.json";

/// A plain run stops while the table differs, so a change shows in a
/// review's diff.
fn lock_kind_table(root: &Path, table: &kind_table::KindTable, locking: bool) {
    let text = serde_json::to_string_pretty(table).expect("the kind table serializes") + "\n";
    let path = root.join(KIND_TABLE);
    if locking {
        if fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            fs::write(&path, &text).expect("write the kind table");
            println!("kind table: {KIND_TABLE} records the kind list");
        }
        return;
    }
    if fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        eprintln!(
            "kind table: {KIND_TABLE} differs from the kind list (crates/cap-abi/src/kinds.rs) and the locked schemas. \
             Run `just schema-lock` to write it again, and review its diff."
        );
        std::process::exit(1);
    }
}

/// The server keeps the schemas in the store (D-202).
fn publish_kind_table(root: &Path, table: &kind_table::KindTable, schemas: &[(String, String)]) {
    let pkg = root.join("web/pkg");
    let dir = pkg.join("schemas");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create web/pkg/schemas");
    for (hash, text) in schemas {
        fs::write(dir.join(format!("{}.json", hash.trim_start_matches("sha256:"))), text).expect("write the schema");
    }
    let json = cap_abi::canonical_bytes(table).expect("the kind table serializes");
    fs::write(pkg.join("kinds.json"), json).expect("write web/pkg/kinds.json");
    println!("kind table: {} kinds, {} schemas (web/pkg/kinds.json)", table.kinds.len(), fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0));
}

/// The `$id` names no host.
fn published_schema(tag: &str, version: u32, schema: &serde_json::Value) -> String {
    let mut schema = schema.clone();
    schema["$id"] = format!("urn:transit-document:{tag}:v{version}").into();
    serde_json::to_string_pretty(&schema).expect("schema serializes") + "\n"
}

fn host_owned_schema(root: &Path, tag: &str, version: u32) -> serde_json::Value {
    let file = root.join(format!("server/schemas/{tag}.v{version}.schema.json"));
    let text = fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    serde_json::from_str(&text).expect("a host-owned schema is JSON")
}

/// The sha256 over `"{path}\n{sha256 hex of its bytes}\n"` for every file
/// that git sees under `crates/`, plus `Cargo.toml` and `Cargo.lock`, in
/// path order. Ignored files are excluded. `caps-run` recomputes it to say
/// when the registry was published from another tree. `None` without git.
fn source_digest(root: &Path) -> Option<String> {
    let listing = Command::new("git")
        .args(["ls-files", "-co", "--exclude-standard", "-z", "--", "crates", "Cargo.toml", "Cargo.lock"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let mut paths: Vec<&str> =
        std::str::from_utf8(&listing.stdout).ok()?.split('\0').filter(|p| !p.is_empty()).collect();
    paths.sort_unstable();
    paths.dedup();
    let mut digest = Sha256::new();
    for path in paths {
        let Ok(bytes) = fs::read(root.join(path)) else { continue };
        digest.update(format!("{path}\n{:x}\n", Sha256::digest(&bytes)).as_bytes());
    }
    Some(format!("sha256:{:x}", digest.finalize()))
}



fn run_examples(caps: &[Linked], capability: &str, op: &str) -> Option<(OpExample, Vec<cap_abi::manifest::KindExample>)> {
    let linked = caps.iter().find(|c| c.declared.id == capability)?;
    let (examples, invoke, invoke_bytes) = (linked.examples, linked.declared.invoke, linked.declared.invoke_bytes);
    let example = examples().into_iter().find(|e| e.op == op)?;
    for load in &example.loads {
        let answer: serde_json::Value = serde_json::from_str(&invoke(&load.to_string())).expect("a load answers JSON");
        assert!(answer.get("ok").is_some(), "{capability}.{op}: a load of its example failed: {answer}");
    }
    let answer = match &example.bytes {
        Some((code, bytes)) => invoke_bytes(*code, bytes),
        None => invoke(&example.request.to_string()),
    };
    let response: serde_json::Value = serde_json::from_str(&answer).expect("an op answers JSON");
    assert!(response.get("ok").is_some(), "{capability}.{op}: its example failed: {response}");
    Some((OpExample { request: example.request, response }, example.kinds))
}

/// The kernel's examples stay with the kernel and the kind list. Each
/// capability gives its own ([`run_examples`]).
fn kernel_example(op: &str, net: Option<&NetworkExampleContext>) -> Option<OpExample> {
    use serde_json::json;
    let (request, response) = match ("kernel", op) {
        ("kernel", "load_walks") => {
            let net = net?;
            let walk = |from: &str, to: &str| transit_streets::walking::Walk { from: from.into(), to: to.into(), walk_secs: 95 };
            let table = transit_streets::walking::WalkTable { kind: Default::default(), schema_version: Default::default(),
                streets_hash: "sha256:…".into(),
                network_hash: net.network_hash.clone(),
                speed_mps: transit_streets::walking::WALK_SPEED_MPS,
                max_walk_secs: transit_streets::walking::MAX_WALK_SECS,
                walks: vec![walk("SA", "SB"), walk("SB", "SA")],
            };
            let walks_hash = cap_abi::canonical_hash(&table).ok()?;
            (
                json!({ "op": "load_walks", "walks": table }),
                json!({ "ok": { "op": "load_walks", "walks_hash": walks_hash } }),
            )
        }
        ("kernel", "load_network") => {
            let ctx = net?;
            (
                json!({ "op": "load_network", "network": ctx.derived }),
                json!({ "ok": { "op": "load_network", "network_hash": ctx.network_hash } }),
            )
        }
        ("kernel", "load_boundary") => {
            let ctx = rt_example_context()?;
            (
                json!({ "op": "load_boundary", "boundary": ctx.boundary }),
                json!({ "ok": { "op": "load_boundary", "boundary_hash": ctx.boundary_hash } }),
            )
        }
        ("kernel", "load_edits") => {
            let edits = transit_sync::EditDocument::new(dispatch_fixture_edits());
            let edits_hash = cap_abi::canonical_hash(&edits).ok()?;
            (
                json!({ "op": "load_edits", "edits": edits }),
                json!({ "ok": { "op": "load_edits", "edits_hash": edits_hash } }),
            )
        }
        ("kernel", "load_live") => {
            let (live, live_hash) = live_example()?;
            (
                json!({ "op": "load_live", "live": live }),
                json!({ "ok": { "op": "load_live", "live_hash": live_hash } }),
            )
        }
        ("kernel", "load_stops") => {
            let stops = fixture_stops().checked().ok()?;
            (
                json!({ "op": "load_stops", "stops": stops }),
                json!({ "ok": { "op": "load_stops", "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "stops": stops.stops.len() } }),
            )
        }
        ("kernel", "load_stop_requests") => {
            let requests = fixture_requests().checked().ok()?;
            (
                json!({ "op": "load_stop_requests", "requests": requests }),
                json!({ "ok": { "op": "load_stop_requests", "requests_hash": cap_abi::canonical_hash(&requests).ok()?, "requests": requests.requests.len() } }),
            )
        }
        ("kernel", "load_service_alerts") => {
            let alerts = fixture_alerts().checked().ok()?;
            (
                json!({ "op": "load_service_alerts", "alerts": alerts }),
                json!({ "ok": { "op": "load_service_alerts", "alerts_hash": cap_abi::canonical_hash(&alerts).ok()?, "alerts": alerts.alerts.len() } }),
            )
        }
        ("kernel", "load_alert_templates") => {
            let templates = fixture_templates().checked().ok()?;
            (
                json!({ "op": "load_alert_templates", "templates": templates }),
                json!({ "ok": { "op": "load_alert_templates", "templates_hash": cap_abi::canonical_hash(&templates).ok()?, "templates": templates.templates.len() } }),
            )
        }
        ("kernel", "load_notes") => {
            let notes = fixture_notes().checked().ok()?;
            (
                json!({ "op": "load_notes", "notes": notes }),
                json!({ "ok": { "op": "load_notes", "notes_hash": cap_abi::canonical_hash(&notes).ok()?, "notes": notes.notes.len() } }),
            )
        }
        ("kernel", "load_work_rules") => {
            let rules = fixture_work_rules().checked().ok()?;
            let work_rules_hash = cap_abi::canonical_hash(&rules).ok()?;
            (
                json!({ "op": "load_work_rules", "work_rules": fixture_work_rules() }),
                json!({ "ok": { "op": "load_work_rules", "work_rules_hash": work_rules_hash, "version": rules.schema_version.0 } }),
            )
        }
        ("kernel", "load_watchlist") => {
            let watchlist = fixture_watchlist();
            let watchlist_hash = cap_abi::canonical_hash(&watchlist).ok()?;
            (
                json!({ "op": "load_watchlist", "document": watchlist }),
                json!({ "ok": { "op": "load_watchlist", "watchlist_hash": watchlist_hash, "version": 2 } }),
            )
        }
        ("kernel", "load_streets") => {
            let ctx = streets_example_context()?;
            (
                json!({ "op": "load_streets", "streets": ctx.streets }),
                json!({ "ok": { "op": "load_streets", "streets_hash": ctx.streets_hash,
                    "ways": ctx.streets.ways.len(), "segments": ctx.segments, "nodes": ctx.nodes } }),
            )
        }
        // A bulk-bytes payload is raw, so its description carries the
        // contract.
        ("kernel", "load_feed") => return None,
        ("kernel", "load_layer") => return None,
        ("kernel", "load_rt") => return None,
        _ => return None,
    };
    Some(OpExample { request, response })
}

