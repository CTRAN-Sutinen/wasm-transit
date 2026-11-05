//! gen-assemblies — write the assembly crates (`assemblies/`): the
//! crates that emit a module's four exports over the capabilities it
//! links (`cap_abi::assembly!`).
//!
//! Two kinds, both generated, never hand-edited:
//!
//! * `module-<cap>` — one capability alone, for every `crates/cap-*`:
//!   what caps-run, agents, the registry's blobs, and the Go server run;
//! * `module-ui-<id>` — one generated UI's capabilities linked into one
//!   module (D-77), for every `web/src/uis/<id>/ui.json`: the
//!   capabilities its `ops` name (`"<cap>.<op>"`), in id order — a
//!   capability's bulk ops carry its index in that order.
//!
//! `just caps` runs it before the build. The crates are GENERATED and
//! gitignored (like the TS bindings): `assemblies/.gitkeep` keeps the
//! directory so the workspace glob resolves in a fresh checkout.

use std::fs;
use std::path::Path;

#[path = "common/assemblies.rs"]
mod common;
use common::{assemblies, files};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let dir = root.join("assemblies");
    let wanted = assemblies(root);

    // An assembly the tree no longer asks for is gone (the directory's
    // .gitkeep stays: the crates are generated, never committed).
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() && !wanted.iter().any(|a| a.name == name) {
                fs::remove_dir_all(entry.path()).expect("remove a retired assembly");
            }
        }
    }
    for assembly in &wanted {
        for (file, content) in files(assembly) {
            let path = dir.join(&assembly.name).join(&file);
            if fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
                fs::create_dir_all(path.parent().unwrap()).expect("assembly dir");
                fs::write(&path, content).expect("write assembly");
            }
        }
        println!("assembly {}: {}", assembly.name, assembly.caps.join(", "));
    }
}
