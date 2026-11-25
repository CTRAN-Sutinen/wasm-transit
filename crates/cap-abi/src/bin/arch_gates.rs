//! The logic is in `common/gates.rs`, and the broken fixtures are in
//! `tests/arch_gates_test.rs`.

use std::fs;
use std::path::Path;
use std::process::Command;

#[path = "common/gates.rs"]
mod gates;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let metadata = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--no-deps", "--offline"])
        .current_dir(root)
        .output()
        .expect("cargo metadata runs");
    assert!(metadata.status.success(), "cargo metadata: {}", String::from_utf8_lossy(&metadata.stderr));
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).expect("cargo metadata answers JSON");
    let packages = gates::packages_from_metadata(&metadata);
    let mut violations = gates::dependency_violations(&packages);

    // The bins are in no module, so the kernel source skips them.
    let kernel = packages.iter().find(|p| p.name == "cap-abi").expect("the kernel is a workspace package");
    let mut kernel_source = String::new();
    let mut stack = vec![root.join("crates/cap-abi/src")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read src").flatten() {
            let path = entry.path();
            if path.is_dir() && path.file_name().is_some_and(|n| n != "bin") {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                kernel_source.push_str(&fs::read_to_string(&path).expect("read source"));
                kernel_source.push('\n');
            }
        }
    }
    violations.extend(gates::kernel_closure_violations(&kernel.required, &kernel_source));

    for package in packages.iter().filter(|p| gates::class(&p.name) == Some(gates::Class::Capability)) {
        let src = root.join("crates").join(&package.name).join("src");
        let lib = fs::read_to_string(src.join("lib.rs")).expect("a capability's src/lib.rs");
        violations.extend(gates::unsafe_violation(&package.name, &lib));
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).expect("read src").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let file = path.strip_prefix(root).unwrap_or(&path).display().to_string();
                    violations.extend(gates::state_violations(&file, &fs::read_to_string(&path).expect("read source")));
                }
            }
        }
    }

    // The allowlists in gates.rs are the exception.
    let ids: Vec<String> = packages
        .iter()
        .filter(|p| gates::class(&p.name) == Some(gates::Class::Capability))
        .map(|p| p.name.trim_start_matches("cap-").to_string())
        .collect();
    let mut files = vec![root.join("Cargo.toml"), root.join("crates/cap-abi/Cargo.toml"), root.join("crates/generator/Cargo.toml")];
    let mut stack = vec![root.join("crates/generator/src"), root.join("crates/cap-abi/src")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read src").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("bin/common/gates.rs") {
                files.push(path);
            }
        }
    }
    for path in files {
        let file = path.strip_prefix(root).unwrap_or(&path).display().to_string();
        violations.extend(gates::capability_name_violations(&file, &fs::read_to_string(&path).expect("read source"), &ids));
    }

    if !violations.is_empty() {
        for v in &violations {
            eprintln!("ARCH FAIL: {v}");
        }
        eprintln!("(crates/AGENTS.md — capabilities never call capabilities; the gates are the rule, never widen them)");
        std::process::exit(1);
    }
    println!("arch gates: {} packages, dependency graph, kernel closure, state and unsafe, capability names — clean", packages.len());
}
