//! The gates keep "capabilities never call capabilities" true after the
//! capabilities link into one binary (D-77, `docs/architecture/bus.md`,
//! "Bus rules"). `arch-gates` runs them in `just test`, and
//! `crates/cap-abi/tests/arch_gates_test.rs` breaks each one on a fixture.

#![allow(dead_code)] // each bin uses part of the module

use std::collections::BTreeMap;

/// Only production dependencies count. Dev-dependencies may build fixtures
/// through anything. `required` holds the dependencies that are not
/// optional, which are always in the closure of the package.
#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub deps: Vec<String>,
    pub required: Vec<String>,
    /// Only these may name the fixtures (D-235).
    pub examples: Vec<String>,
}

/// A package in none of these classes fails the gate until the gate learns
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Kernel,
    Capability,
    Library,
    Assembly,
    /// No module links it. It names no capability, because the capabilities
    /// reach it as data.
    Generator,
    GeneratorBinary,
    /// No module links it (D-235).
    Fixtures,
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
    } else if name == "generator" {
        Some(Class::Generator)
    } else if name == "gen-manifest" {
        Some(Class::GeneratorBinary)
    } else if name == "fixtures" {
        Some(Class::Fixtures)
    } else {
        None
    }
}

/// The packages that may read transit-codec in production code (Law 1a).
/// rt is included, because its updates number stops as the published feed
/// does. The generator and its fixtures compute the examples of the codec.
pub const CODEC_HOMES: &[&str] = &["transit-codec", "cap-gtfs", "cap-network", "cap-rt", "transit-rt", "generator", "fixtures"];

/// Gate 1 denies by default. The kernel depends on transit-version alone
/// (D-239), and so does the network. Only [`CODEC_HOMES`] and the assemblies
/// that link them read the GTFS layer.
pub fn dependency_violations(packages: &[Package]) -> Vec<String> {
    let mut out = Vec::new();
    let classes: BTreeMap<&str, Option<Class>> = packages.iter().map(|p| (p.name.as_str(), class(&p.name))).collect();
    for package in packages {
        let Some(own) = class(&package.name) else {
            out.push(format!("{}: not a kernel, capability, library, assembly, generator, generator binary, or fixtures — the gate denies by default", package.name));
            continue;
        };
        for dep in &package.deps {
            let Some(dep_class) = classes.get(dep.as_str()).copied().flatten() else {
                continue; // An external crate.
            };
            let allowed = match (own, dep_class) {
                (Class::Assembly, Class::Capability | Class::Kernel) => true,
                (Class::Capability, Class::Kernel | Class::Library) => true,
                (Class::Kernel | Class::Library, Class::Library) => true,
                (Class::Generator, Class::Kernel | Class::Library | Class::Fixtures) => true,
                (Class::GeneratorBinary, Class::Generator | Class::Capability) => true,
                (Class::Fixtures, Class::Kernel | Class::Library) => true,
                // No module build turns on the `examples` feature.
                (Class::Capability, Class::Fixtures) => !package.required.contains(dep) && package.examples.contains(dep),
                _ => false,
            };
            if !allowed {
                out.push(format!("{} ({own:?}) depends on {dep} ({dep_class:?}): {}", package.name, rule(own)));
            }
            if own == Class::Kernel && dep_class == Class::Library && dep != "transit-version" {
                out.push(format!(
                    "cap-abi depends on {dep}: the kernel knows no kind and depends on transit-version alone; the kind's library implements transit_version::Load (D-239)"
                ));
            }
            if package.name == "transit-network" && dep != "transit-version" {
                out.push(format!(
                    "transit-network depends on {dep}: the network is the core model and depends only on transit-version; a library that joins a document to the network depends on the network (crates/AGENTS.md, Law 1)"
                ));
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
        Class::Capability => "a capability depends only on cap-abi and libraries — capabilities never call capabilities — and on the fixtures only through its `examples` feature",
        Class::Library => "a library depends only on libraries",
        Class::Kernel => "the kernel depends only on libraries",
        Class::Assembly => "an assembly links capabilities over the kernel",
        Class::Generator => "the generator reads the kernel, the libraries, and the fixtures; the capabilities reach it as data, through its binary",
        Class::GeneratorBinary => "the generator's binary links the generator and the capabilities",
        Class::Fixtures => "the fixtures read the kernel and libraries",
    }
}

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
                .filter(|d| d["kind"].is_null()) // Dev and build dependencies are not production code.
                .filter(|d| d["name"].as_str().is_some_and(|n| names.iter().any(|w| w == n)))
                .collect();
            let name = |d: &&serde_json::Value| d["name"].as_str().unwrap_or_default().to_string();
            let examples = p["features"]["examples"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f.as_str())
                .map(|f| f.strip_prefix("dep:").unwrap_or(f).to_string())
                .collect();
            Package {
                name: p["name"].as_str().unwrap_or_default().to_string(),
                deps: normal.iter().map(name).collect(),
                required: normal.iter().filter(|d| d["optional"].as_bool() != Some(true)).map(name).collect(),
                examples,
            }
        })
        .collect()
}

/// Gate 4. A library that the kernel requires is in every module, and a
/// change in it moves the hash of every module, so every client downloads
/// every module again. The kernel therefore requires only the libraries
/// that its code names. `kernel_source` is the source of the cap-abi
/// library, without its bins.
pub fn kernel_closure_violations(required: &[String], kernel_source: &str) -> Vec<String> {
    let code: String = kernel_source.lines().map(|l| l.split("//").next().unwrap_or_default()).collect::<Vec<_>>().join("\n");
    required
        .iter()
        .filter(|d| class(d) == Some(Class::Library))
        .filter(|d| !names_crate(&code, &d.replace('-', "_")))
        .map(|d| {
            format!(
                "cap-abi requires {d}, which its library never names: every module would carry it"
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

/// Gate 3. The `assembly!` macro holds the unsafe exports and expands only
/// in assembly crates.
pub fn unsafe_violation(crate_name: &str, lib_rs: &str) -> Option<String> {
    let forbids = lib_rs.lines().any(|line| line.trim() == "#![forbid(unsafe_code)]");
    (!forbids).then(|| format!("{crate_name}: src/lib.rs does not #![forbid(unsafe_code)] — the unsafe exports are the kernel's"))
}

/// The other half of Gate 2. `capability!` declares the one static of the
/// capability, so the source holds no other.
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

/// Gate 5. A capability's crate. Its bare id is not matched: ids such as
/// `requests` and `alerts` are also field names. Comments do not count,
/// and `#` is a comment in a `.toml` (D-243).
pub fn capability_name_violations(file: &str, source: &str, ids: &[String]) -> Vec<String> {
    let comment = if file.ends_with(".toml") { "#" } else { "//" };
    let mut out = Vec::new();
    for (n, line) in source.lines().enumerate() {
        let code = line.split(comment).next().unwrap_or_default().replace('-', "_");
        for id in ids {
            if names_crate(&code, &format!("cap_{}", id.replace('-', "_"))) {
                out.push(format!("{file}:{}: names cap-{id} — a capability is its crate, and nothing hand-written lists it", n + 1));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn quotes(code: &str, word: &str) -> bool {
    ['"', '\'', '`'].iter().any(|q| code.contains(&format!("{q}{word}{q}")))
}

/// The identifiers of a kind that the kind table routes generically: a
/// `post` route answered by its log's receive op, or a `work_order` route
/// (D-254). The plumbing reads such a kind from the table.
#[derive(Debug)]
pub struct RoutedKind {
    pub kind: String,
    /// Names that count as a string literal.
    pub names: Vec<String>,
    /// Identifiers that count anywhere in code: the load op, the hash
    /// fields.
    pub words: Vec<String>,
}

pub fn routed_kinds(table: &serde_json::Value) -> Vec<RoutedKind> {
    table["kinds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|k| !k["post"].is_null() || !k["work_order"].is_null())
        .map(|k| {
            let text = |v: &serde_json::Value| v.as_str().unwrap_or_default().to_string();
            let kind = text(&k["store_kind"]);
            let mut names = vec![kind.clone(), text(&k["tag"])];
            let mut words = vec![text(&k["load"]["op"]), text(&k["post"]["answer"]), format!("{kind}_hash"), format!("{}_hash", text(&k["tag"]))];
            names.retain(|n| !n.is_empty());
            names.dedup();
            words.retain(|w| !w.is_empty() && w != "_hash");
            words.sort();
            words.dedup();
            RoutedKind { kind, names, words }
        })
        .collect()
}

/// Gate 6. The kernel, the server, the runner, and the engine name no kind
/// that the table routes generically. Comments do not count.
pub fn kind_name_violations(file: &str, source: &str, kinds: &[RoutedKind]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for k in kinds {
            if k.names.iter().any(|name| quotes(code, name)) || k.words.iter().any(|w| names_crate(code, w)) {
                out.push(format!("{file}:{}: names the {} kind — the kind table routes it, so the plumbing reads it from the table", n + 1, k.kind));
            }
        }
    }
    out
}
