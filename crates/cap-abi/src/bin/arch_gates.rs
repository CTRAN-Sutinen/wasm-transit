//! arch-gates — the architecture gates over the workspace as it is
//! (`just test`): the dependency graph (`cargo metadata`), the kernel's
//! closure (only the libraries its code names), and every capability
//! crate's source (no state of its own, `unsafe` forbidden).
//! The gates' logic and its broken fixtures: `common/gates.rs`,
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

    // The kernel's closure: its library's source (the generators' bins
    // are not in any module).
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

    if !violations.is_empty() {
        for v in &violations {
            eprintln!("ARCH FAIL: {v}");
        }
        eprintln!("(crates/AGENTS.md — capabilities never call capabilities; the gates are the rule, never widen them)");
        std::process::exit(1);
    }
    println!("arch gates: {} packages, dependency graph, kernel closure, state and unsafe — clean", packages.len());
}
