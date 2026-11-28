//! The Versions table of docs/FORMATS.md and the schema locks name the same
//! kinds. A kind in the table with no schema names its reason in `NOT_LOCKED`.

use std::collections::BTreeSet;
use std::path::Path;

const NOT_LOCKED: &[(&str, &str)] = &[
    (
        "diversion_draft",
        "never persisted yet: it gains its tags when something first persists one",
    ),
    (
        "manifest",
        "its schema is the HTTP contract's (server/openapi.json)",
    ),
    (
        "build_file",
        "a build file is its exact bytes, of whatever type the file is",
    ),
];

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn inventory() -> BTreeSet<String> {
    let text = std::fs::read_to_string(root().join("docs/FORMATS.md")).unwrap();
    let start = text
        .find("\n## Versions\n")
        .expect("docs/FORMATS.md has a \"## Versions\" section");
    let section = &text[start + 1..];
    let section = section[3..]
        .find("\n## ")
        .map_or(section, |end| &section[..end + 3]);
    let mut out = BTreeSet::new();
    for line in section.lines().filter(|l| l.starts_with("| `")) {
        let first = line.split('|').nth(1).unwrap_or_default();
        for name in first.split('`').skip(1).step_by(2) {
            out.insert(name.trim().to_string());
        }
    }
    out
}

fn locked() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for owner in std::fs::read_dir(root().join("crates")).unwrap().flatten() {
        for file in std::fs::read_dir(owner.path().join("locks"))
            .into_iter()
            .flatten()
            .flatten()
        {
            if let Some(tag) = file
                .file_name()
                .to_string_lossy()
                .strip_suffix(".lock.json")
            {
                out.insert(tag.to_string());
            }
        }
    }
    for file in std::fs::read_dir(root().join("server/schemas"))
        .unwrap()
        .flatten()
    {
        let name = file.file_name().to_string_lossy().to_string();
        if let Some(stem) = name.strip_suffix(".schema.json") {
            out.insert(stem.split(".v").next().unwrap().to_string());
        }
    }
    out
}

#[test]
fn the_inventory_and_the_locks_name_the_same_kinds() {
    let (inventory, locked) = (inventory(), locked());
    let exempt: BTreeSet<String> = NOT_LOCKED.iter().map(|(k, _)| k.to_string()).collect();
    let unlocked: Vec<_> = inventory
        .difference(&locked)
        .filter(|k| !exempt.contains(*k))
        .collect();
    assert!(
        unlocked.is_empty(),
        "docs/FORMATS.md lists kinds with no locked schema: {unlocked:?}"
    );
    let unlisted: Vec<_> = locked.difference(&inventory).collect();
    assert!(
        unlisted.is_empty(),
        "locked kinds docs/FORMATS.md, \"Versions\" does not list: {unlisted:?}"
    );
    let stale: Vec<_> = exempt
        .iter()
        .filter(|k| !inventory.contains(*k) || locked.contains(*k))
        .collect();
    assert!(
        stale.is_empty(),
        "exemptions to drop (no longer listed, or now locked): {stale:?}"
    );
}
