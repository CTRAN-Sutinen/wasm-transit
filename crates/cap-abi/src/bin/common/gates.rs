//! The architecture gates that keep "capabilities never call
//! capabilities" true once they link into one binary (D-77,
//! `docs/architecture/bus.md`, "Bus rules"): in one binary a crate can still
//! name only the code of the crates it depends on, so the rule is the
//! dependency graph, the kernel's private registry constructor, and no
//! `unsafe` outside the kernel — and the kernel's closure kept to the
//! libraries it uses, so a change elsewhere moves only the modules that
//! carry it (docs/architecture/release-artifacts.md). `arch-gates`
//! runs them in `just test`;
//! `crates/cap-abi/tests/arch_gates_test.rs` breaks each on a fixture.

#![allow(dead_code)] // each bin uses part of the module

use std::collections::BTreeMap;

/// One workspace package as the gates read it: its name and its
/// NORMAL dependencies on other workspace packages (dev-dependencies
/// may build fixtures through anything; production code may not) —
/// all of them, and the ones that are not optional (always in the
/// package's closure).
#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub deps: Vec<String>,
    pub required: Vec<String>,
}

/// What a workspace package is, by name — deny by default: a package
/// that is none of these fails the gate until the gate learns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// `cap-abi`: the kernel and the ABI.
    Kernel,
    /// `cap-*`: a capability.
    Capability,
    /// `transit-*`: a library.
    Library,
    /// `module-*`: a generated assembly.
    Assembly,
}

pub fn class(name: &str) -> Option<Class> {
    if name == "cap-abi" {
        Some(Class::Kernel)
    } else if name.starts_with("cap-") {
        Some(Class::Capability)
    } else if name.starts_with("transit-") {
        Some(Class::Library)
    } else if name.starts_with("module-") {
        Some(Class::Assembly)
    } else {
        None
    }
}

/// The boundary's homes: the packages that may read the GTFS layer
/// (transit-codec) in production code — Law 1a. The codec itself, the
/// boundary capabilities, rt (its updates number stops as the
/// published feed does), and the kernel (it registers feeds and
/// boundaries, and gen-manifest computes the codec's examples).
pub const CODEC_HOMES: &[&str] = &["transit-codec", "cap-gtfs", "cap-network", "cap-rt", "transit-rt", "cap-abi"];

/// Gate 1: the dependency graph, deny by default. A capability depends
/// only on the kernel and libraries; a library on libraries; the kernel
/// on libraries; only an assembly depends on capabilities; and only
/// the codec's homes (and assemblies, which link them) read the GTFS
/// layer.
pub fn dependency_violations(packages: &[Package]) -> Vec<String> {
    let mut out = Vec::new();
    let classes: BTreeMap<&str, Option<Class>> = packages.iter().map(|p| (p.name.as_str(), class(&p.name))).collect();
    for package in packages {
        let Some(own) = class(&package.name) else {
            out.push(format!("{}: not a kernel, capability, library or assembly — the gate denies by default", package.name));
            continue;
        };
        for dep in &package.deps {
            let Some(dep_class) = classes.get(dep.as_str()).copied().flatten() else {
                continue; // not a workspace package: an external crate
            };
            let allowed = match (own, dep_class) {
                (Class::Assembly, Class::Capability | Class::Kernel) => true,
                (Class::Capability, Class::Kernel | Class::Library) => true,
                (Class::Kernel | Class::Library, Class::Library) => true,
                _ => false,
            };
            if !allowed {
                out.push(format!("{} ({own:?}) depends on {dep} ({dep_class:?}): {}", package.name, rule(own)));
            }
            if dep == "transit-codec" && own != Class::Assembly && !CODEC_HOMES.contains(&package.name.as_str()) {
                out.push(format!(
                    "{} depends on transit-codec: the network is the core model — compose over the network document (crates/AGENTS.md, Law 1)",
                    package.name
                ));
            }
        }
    }
    out
}

fn rule(class: Class) -> &'static str {
    match class {
        Class::Capability => "a capability depends only on cap-abi and libraries — capabilities never call capabilities",
        Class::Library => "a library depends only on libraries",
        Class::Kernel => "the kernel depends only on libraries",
        Class::Assembly => "an assembly links capabilities over the kernel",
    }
}

/// The workspace's packages from `cargo metadata --no-deps` JSON.
pub fn packages_from_metadata(metadata: &serde_json::Value) -> Vec<Package> {
    let names: Vec<String> = metadata["packages"]
        .as_array()
        .map(|ps| ps.iter().filter_map(|p| p["name"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let normal: Vec<&serde_json::Value> = p["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["kind"].is_null()) // normal; "dev" and "build" are not production code
                .filter(|d| d["name"].as_str().is_some_and(|n| names.iter().any(|w| w == n)))
                .collect();
            let name = |d: &&serde_json::Value| d["name"].as_str().unwrap_or_default().to_string();
            Package {
                name: p["name"].as_str().unwrap_or_default().to_string(),
                deps: normal.iter().map(name).collect(),
                required: normal.iter().filter(|d| d["optional"].as_bool() != Some(true)).map(name).collect(),
            }
        })
        .collect()
}

/// Gate 4: the kernel's closure. Every module links the kernel, so a
/// library the kernel requires is in every module's closure, and a
/// dependency or feature change in it moves every module's hash (every
/// client downloads every module again). The kernel requires only the
/// libraries its own code (`kernel_source`: its library's source, the
/// generators' bins excluded) names; a generator's extra libraries are
/// optional, behind its feature.
pub fn kernel_closure_violations(required: &[String], kernel_source: &str) -> Vec<String> {
    let code: String = kernel_source.lines().map(|l| l.split("//").next().unwrap_or_default()).collect::<Vec<_>>().join("\n");
    required
        .iter()
        .filter(|d| class(d) == Some(Class::Library))
        .filter(|d| !names_crate(&code, &d.replace('-', "_")))
        .map(|d| {
            format!(
                "cap-abi requires {d}, which its library never names: every module would carry it — make it an optional dependency of the generator that needs it"
            )
        })
        .collect()
}

fn names_crate(code: &str, ident: &str) -> bool {
    let ident_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    code.match_indices(ident).any(|(at, _)| {
        !code[..at].chars().next_back().is_some_and(ident_char) && !code[at + ident.len()..].chars().next().is_some_and(ident_char)
    })
}

/// Gate 3: a capability crate forbids `unsafe` (the unsafe exports live
/// in the kernel's `assembly!`, expanded only in assembly crates).
pub fn unsafe_violation(crate_name: &str, lib_rs: &str) -> Option<String> {
    let forbids = lib_rs.lines().any(|line| line.trim() == "#![forbid(unsafe_code)]");
    (!forbids).then(|| format!("{crate_name}: src/lib.rs does not #![forbid(unsafe_code)] — the unsafe exports are the kernel's"))
}

/// Gate 2's other half: a capability declares no state of its own —
/// no `static` (the kernel's registries are the only shared state, and
/// their constructor is private to the kernel), no `thread_local!`, no
/// lazy statics. `capability!` declares the capability's one static.
pub fn state_violations(file: &str, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (n, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("").trim_start();
        let item = code.strip_prefix("pub(crate) ").or_else(|| code.strip_prefix("pub ")).unwrap_or(code);
        if item.starts_with("static ") || code.contains("thread_local!") || code.contains("lazy_static!") {
            out.push(format!("{file}:{}: {} — a capability declares no state; documents live in the kernel's registries", n + 1, line.trim()));
        }
    }
    out
}
