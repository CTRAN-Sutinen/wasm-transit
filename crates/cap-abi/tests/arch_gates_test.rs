//! The architecture gates, each broken on a deliberate fixture: a gate
//! that cannot fail proves nothing. The gates themselves run over the
//! real workspace in `just test` (`arch-gates`); the registry
//! constructor's privacy is a `compile_fail` doctest on
//! `cap_abi::registry::Registry` (a capability that declares a
//! registry does not compile).

#[path = "../src/bin/common/gates.rs"]
mod gates;

use gates::{capability_name_violations, dependency_violations, kernel_closure_violations, state_violations, unsafe_violation, Package};

fn package(name: &str, deps: &[&str]) -> Package {
    let deps: Vec<String> = deps.iter().map(|d| d.to_string()).collect();
    Package { name: name.into(), required: deps.clone(), deps, examples: vec![] }
}

/// A workspace that keeps the rule: capabilities over the kernel and
/// libraries, assemblies over capabilities.
fn lawful() -> Vec<Package> {
    vec![
        package("cap-abi", &["transit-version"]),
        package("transit-version", &[]),
        package("transit-network", &["transit-version"]),
        package("transit-codec", &["transit-network"]),
        package("transit-dispatch", &["transit-network"]),
        package("cap-board", &["cap-abi", "transit-dispatch"]),
        package("cap-network", &["cap-abi", "transit-codec"]),
        package("module-board", &["cap-abi", "cap-board"]),
        package("module-ui-stop-display", &["cap-abi", "cap-board", "cap-network"]),
    ]
}

#[test]
fn the_lawful_workspace_passes() {
    assert_eq!(dependency_violations(&lawful()), Vec::<String>::new());
}

/// The generator and its fixtures (D-235): the generator reads the
/// kernel, the libraries, and the fixtures, and names no capability;
/// its generated binary links it and the capabilities; the fixtures read
/// the kernel and the libraries; a capability names the fixtures only
/// through its `examples` feature; only the binary depends on the
/// generator, so no module's closure holds either.
#[test]
fn only_the_generator_depends_on_the_fixtures_and_nothing_on_the_generator() {
    let mut ws = lawful();
    ws.push(package("fixtures", &["cap-abi", "transit-network"]));
    ws.push(package("generator", &["cap-abi", "fixtures", "transit-codec"]));
    ws.push(package("gen-manifest", &["generator", "cap-board"]));
    assert_eq!(dependency_violations(&ws), Vec::<String>::new());
    // A capability names the fixtures only as an optional dependency its
    // `examples` feature turns on.
    let mut examples = ws.clone();
    examples.retain(|p| p.name != "cap-board");
    examples.push(Package { name: "cap-board".into(), deps: vec!["cap-abi".into(), "fixtures".into()], required: vec!["cap-abi".into()], examples: vec!["fixtures".into()] });
    assert_eq!(dependency_violations(&examples), Vec::<String>::new());
    let mut other_feature = examples.clone();
    other_feature.last_mut().unwrap().examples.clear();
    assert!(dependency_violations(&other_feature).iter().any(|v| v.starts_with("cap-board")), "an optional dependency another feature turns on");
    for (name, deps) in [
        ("cap-board", &["cap-abi", "fixtures"][..]),
        ("module-board", &["cap-board", "generator"][..]),
        ("fixtures", &["cap-board"][..]),
        ("generator", &["cap-abi", "cap-board"][..]),
        ("gen-manifest", &["generator", "fixtures"][..]),
    ] {
        let mut bad = ws.clone();
        bad.retain(|p| p.name != name);
        bad.push(package(name, deps));
        let found = dependency_violations(&bad);
        assert!(found.iter().any(|v| v.starts_with(name)), "{name} -> {deps:?}: {found:?}");
    }
}

/// The kernel knows no kind (D-239): each kind's library loads its own
/// kind, so the kernel depends on transit-version alone.
#[test]
fn a_kernel_that_depends_on_a_domain_library_fails() {
    let mut ws = lawful();
    ws.retain(|p| p.name != "cap-abi");
    ws.push(package("cap-abi", &["transit-version", "transit-network"]));
    let found = dependency_violations(&ws);
    assert!(found.iter().any(|v| v.starts_with("cap-abi") && v.contains("transit-network")), "{found:?}");
}

#[test]
fn a_capability_that_depends_on_a_capability_fails() {
    let mut ws = lawful();
    ws.push(package("cap-router", &["cap-abi", "cap-board"]));
    let found = dependency_violations(&ws);
    assert!(found.iter().any(|v| v.contains("cap-router") && v.contains("cap-board")), "{found:?}");
}

#[test]
fn a_library_that_depends_on_a_capability_or_the_kernel_fails() {
    let mut ws = lawful();
    ws.push(package("transit-live", &["cap-board"]));
    ws.push(package("transit-rt", &["cap-abi"]));
    let found = dependency_violations(&ws);
    assert!(found.iter().any(|v| v.starts_with("transit-live")), "{found:?}");
    assert!(found.iter().any(|v| v.starts_with("transit-rt")), "{found:?}");
}

#[test]
fn the_kernel_never_depends_on_a_capability() {
    let mut ws = lawful();
    ws[0].deps.push("cap-board".into());
    assert!(dependency_violations(&ws).iter().any(|v| v.starts_with("cap-abi")));
}

#[test]
fn a_model_side_crate_that_reads_the_gtfs_layer_fails() {
    let mut ws = lawful();
    ws.push(package("cap-calendar", &["cap-abi", "transit-codec"]));
    ws.push(package("transit-analysis", &["transit-codec"]));
    let found = dependency_violations(&ws);
    assert!(found.iter().any(|v| v.starts_with("cap-calendar depends on transit-codec")), "{found:?}");
    assert!(found.iter().any(|v| v.starts_with("transit-analysis depends on transit-codec")), "{found:?}");
}

#[test]
fn the_network_depending_on_a_document_library_fails() {
    let mut ws = lawful();
    ws.push(package("transit-stops", &["transit-network"]));
    assert_eq!(dependency_violations(&ws), Vec::<String>::new());
    for p in ws.iter_mut().filter(|p| p.name == "transit-network") {
        p.deps.push("transit-stops".into());
        p.required.push("transit-stops".into());
    }
    let found = dependency_violations(&ws);
    assert!(found.iter().any(|v| v.starts_with("transit-network depends on transit-stops")), "{found:?}");
}

#[test]
fn an_unknown_package_is_denied_by_default() {
    let mut ws = lawful();
    ws.push(package("helpers", &[]));
    assert!(dependency_violations(&ws).iter().any(|v| v.contains("denies by default")));
}

#[test]
fn the_gate_reads_normal_dependencies_from_cargo_metadata() {
    let metadata = serde_json::json!({ "packages": [
        { "name": "cap-board", "dependencies": [
            { "name": "cap-abi", "kind": null },
            { "name": "cap-router", "kind": null },
            { "name": "transit-codec", "kind": "dev" },
            { "name": "serde", "kind": null },
        ] },
        { "name": "cap-abi", "dependencies": [] },
        { "name": "cap-router", "dependencies": [] },
        { "name": "transit-codec", "dependencies": [] },
    ] });
    let packages = gates::packages_from_metadata(&metadata);
    assert_eq!(packages[0].deps, vec!["cap-abi".to_string(), "cap-router".to_string()], "dev and external deps are not the rule's");
    assert_eq!(packages[0].required, packages[0].deps, "none of them optional");
    assert!(dependency_violations(&packages).iter().any(|v| v.contains("cap-board") && v.contains("cap-router")));
}

#[test]
fn a_capability_that_allows_unsafe_fails() {
    assert!(unsafe_violation("cap-board", "//! the board\n\n#![forbid(unsafe_code)]\n\nuse x;").is_none());
    assert!(unsafe_violation("cap-board", "//! the board\n\nuse x;\nunsafe fn f() {}").is_some());
    assert!(unsafe_violation("cap-board", "// #![forbid(unsafe_code)]").is_some(), "a comment is not the attribute");
}

#[test]
fn a_capability_that_declares_state_fails() {
    assert!(state_violations("ok.rs", "fn f(x: &'static str) {}\nconst N: u32 = 3;\n// static in prose").is_empty());
    for broken in [
        "static CACHE: Mutex<HashMap<String, Network>> = Mutex::new(HashMap::new());",
        "pub static NETWORKS: Registry<Network> = todo!();",
        "pub(crate) static SEEN: AtomicU32 = AtomicU32::new(0);",
        "thread_local! { static DAY: RefCell<Vec<u32>> = RefCell::new(vec![]); }",
    ] {
        assert_eq!(state_violations("cap.rs", broken).len(), 1, "{broken}");
    }
}

// HASH CHURN (docs/architecture/release-artifacts.md): every module
// links the kernel, so a library the kernel depends on is in every
// module's closure — a dependency or feature change in it moves every
// module's hash. The kernel requires only the libraries its own code
// names (and gate 1 holds that set to transit-version).

#[test]
fn the_kernel_requires_only_the_libraries_its_code_names() {
    let kernel = "use transit_network::Network;\nfn f() { transit_codec::import(); }\n// transit_router in prose";
    let required = |deps: &[&str]| deps.iter().map(|d| d.to_string()).collect::<Vec<_>>();
    assert_eq!(kernel_closure_violations(&required(&["transit-network", "transit-codec", "serde"]), kernel), Vec::<String>::new());
    let found = kernel_closure_violations(&required(&["transit-network", "transit-router", "transit-plans"]), kernel);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().any(|v| v.starts_with("cap-abi requires transit-router")), "a name in a comment is not a use: {found:?}");
    assert!(found.iter().any(|v| v.starts_with("cap-abi requires transit-plans")), "{found:?}");
    let prefix = kernel_closure_violations(&required(&["transit-net"]), kernel);
    assert_eq!(prefix.len(), 1, "a prefix of a name is not the name: {prefix:?}");
}

#[test]
fn the_gate_reads_which_dependencies_are_optional() {
    let metadata = serde_json::json!({ "packages": [
        { "name": "cap-abi", "dependencies": [
            { "name": "transit-network", "kind": null },
            { "name": "transit-router", "kind": null, "optional": true },
            { "name": "transit-plans", "kind": "dev" },
        ] },
        { "name": "transit-network", "dependencies": [] },
        { "name": "transit-router", "dependencies": [] },
        { "name": "transit-plans", "dependencies": [] },
    ] });
    let packages = gates::packages_from_metadata(&metadata);
    assert_eq!(packages[0].deps, vec!["transit-network".to_string(), "transit-router".to_string()], "the graph rule reads optional deps too");
    assert_eq!(packages[0].required, vec!["transit-network".to_string()], "the closure rule reads only the required ones");
}

/// No hand-written list names a capability (D-243): the generator and
/// the kernel take them as data.
#[test]
fn a_list_that_names_a_capability_fails() {
    let ids = vec!["board".to_string(), "rt".to_string()];
    let clean = "fn f(caps: &[Linked]) {}\n// cap-board in prose\nlet s = \"module_{id}.wasm\";\nuse cap_abi::kernel;";
    assert_eq!(capability_name_violations("crates/generator/src/lib.rs", clean, &ids), Vec::<String>::new());
    let found = capability_name_violations("crates/generator/src/lib.rs", "match id { \"x\" => cap_board::invoke, _ => cap_rt::invoke }", &ids);
    assert_eq!(found.len(), 2, "{found:?}");
    let toml = "[dependencies]\ncap-board = { path = \"../cap-board\" }\n# cap-rt in prose";
    let found = capability_name_violations("crates/cap-abi/Cargo.toml", toml, &ids);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("cap-board"), "{found:?}");
}
