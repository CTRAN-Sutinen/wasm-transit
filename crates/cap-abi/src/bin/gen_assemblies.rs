//! gen-assemblies writes the assembly crates (`assemblies/`) and gen-manifest,
//! the generator's binary. Nobody edits them by hand, and git ignores them.
//!
//! `module-<cap>` links one capability alone, for caps-run, agents, and the
//! registry's blobs. `module-ui-<id>` links the capabilities that the `ops`
//! of a UI's `ui.json` name (D-77), in id order. `module-server` links the
//! Go server's (D-227).
//! `assemblies/.gitkeep` keeps the directory, so the workspace glob resolves
//! in a fresh checkout.

use std::fs;
use std::path::Path;

#[path = "common/assemblies.rs"]
mod common;
use common::{assemblies, files, generator_files, GENERATOR};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let dir = root.join("assemblies");
    let wanted = assemblies(root);

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() && name != GENERATOR && !wanted.iter().any(|a| a.name == name) {
                fs::remove_dir_all(entry.path()).expect("remove a retired assembly");
            }
        }
    }
    let write = |crate_dir: &str, files: Vec<(String, String)>| {
        for (file, content) in files {
            let path = dir.join(crate_dir).join(&file);
            if fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
                fs::create_dir_all(path.parent().unwrap()).expect("assembly dir");
                fs::write(&path, content).expect("write assembly");
            }
        }
    };
    for assembly in &wanted {
        write(&assembly.name, files(assembly));
        println!("assembly {}: {}", assembly.name, assembly.caps.join(", "));
    }
    write(GENERATOR, generator_files(root));
    println!("generator {GENERATOR}: every capability");
}
