//! The schema lock: what each document kind has promised
//! (docs/FORMATS.md, "The schema lock"). For each kind and each version
//! it froze, the owning crate keeps three files in `locks/`:
//!
//! - `<tag>.v<N>.schema.json`: the JSON Schema of that version.
//! - `<tag>.v<N>.example.json`: a worked example, in its canonical
//!   bytes (RFC 8785).
//! - `<tag>.lock.json`: the index, with the hash of each file.
//!
//! `<tag>` is the kind's tag, the name a document carries inside it.
//! [`check`] compares a build with its lock. [`freeze`] writes what a
//! build adds (a new version, or an added optional field, D-167) and
//! refuses every other change. gen-manifest runs both; the test is
//! `crates/generator/tests/schema_lock_test.rs`.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde_json::{json, Value};

/// Read a document as the current type and write it again: its
/// canonical bytes, and the names of the fields the type does not know.
pub type Reparse = fn(&Value) -> Result<(String, Vec<String>), String>;

/// Bring a document of an older version (the second argument) to the
/// current one.
pub type Migrate = fn(Value, u32) -> Result<Value, String>;

/// One document kind as this build reads and writes it.
pub struct Kind {
    /// The tag a document of the kind carries inside it.
    pub tag: &'static str,
    /// The version this build writes.
    pub current: u32,
    /// The JSON Schema generated from the current serde type.
    pub schema: Value,
    /// A worked example of the current version, computed through the
    /// owning library. [`freeze`] freezes it.
    pub example: Option<Value>,
    pub reparse: Reparse,
    /// The kind's migration chain. A kind with no older version needs
    /// none.
    pub migrate: Option<Migrate>,
}

/// A kind whose current serde type is `T`.
pub fn kind<T>(tag: &'static str, current: u32, example: Option<Value>, migrate: Option<Migrate>) -> Kind
where
    T: schemars::JsonSchema + serde::Serialize + serde::de::DeserializeOwned,
{
    let schema = serde_json::to_value(schemars::schema_for!(T)).expect("a schema serializes");
    Kind { tag, current, schema, example, reparse: reparse::<T>, migrate }
}

fn reparse<T: serde::Serialize + serde::de::DeserializeOwned>(doc: &Value) -> Result<(String, Vec<String>), String> {
    let (read, unknown) = transit_version::noting_unknown(|| serde_json::from_value::<T>(doc.clone()));
    let read = read.map_err(|e| e.to_string())?;
    Ok((cap_abi::canonical_bytes(&read)?, unknown))
}

/// One frozen version of a kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Locked {
    pub schema: Value,
    /// The example's canonical bytes.
    pub example: String,
    /// The hashes the index records. A lock read from files compares
    /// them with the files.
    pub schema_hash: String,
    pub example_hash: String,
}

impl Locked {
    fn new(schema: Value, example: String) -> Self {
        let schema_hash = cap_abi::content_hash(schema_text(&schema).as_bytes());
        let example_hash = cap_abi::content_hash(example.as_bytes());
        Locked { schema, example, schema_hash, example_hash }
    }
}

/// Every frozen version of one kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Lock {
    pub tag: String,
    pub versions: BTreeMap<u32, Locked>,
}

impl Lock {
    pub fn empty(tag: &str) -> Self {
        Lock { tag: tag.into(), versions: BTreeMap::new() }
    }
}

/// One way a build differs from its lock.
#[derive(Debug, Clone, PartialEq)]
pub enum Drift {
    /// The build writes a version that is not frozen.
    Unlocked { tag: String, version: u32 },
    /// The generated schema differs from the frozen one. It is additive
    /// when the frozen example still reads, knows every field, and
    /// writes the same bytes.
    SchemaChanged { tag: String, version: u32, additive: bool },
    /// The frozen example does not read as the current type.
    Unreadable { tag: String, version: u32, error: String },
    /// The frozen example has fields the current type does not know: a
    /// field was removed or renamed.
    UnknownFields { tag: String, version: u32, names: Vec<String> },
    /// The frozen example writes other bytes, so every document of the
    /// kind changes its identity.
    HashMoved { tag: String, version: u32, was: String, now: String },
    /// A lock file does not match the hash its index records, or the
    /// example is not in canonical form.
    Edited { tag: String, version: u32, file: String },
    /// An older version's frozen example does not migrate to the
    /// current version.
    MigrationBroken { tag: String, from: u32, error: String },
    /// A version newer than the build is frozen.
    LockAhead { tag: String, version: u32, current: u32 },
    /// The build has no worked example to freeze.
    NoExample { tag: String, version: u32 },
    /// A field's declared target changed, or its declaration is gone
    /// (D-232). A changed target changes what the field means, even when
    /// no byte changes. An added declaration is additive.
    ReferenceChanged { tag: String, version: u32, path: String, was: String, now: Option<String> },
}

const BUMP: &str = "Bump the version and add its migration step, then run `just schema-lock`.";

impl fmt::Display for Drift {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Drift::Unlocked { tag, version } => {
                write!(f, "{tag} v{version} is not frozen. Run `just schema-lock` to freeze it.")
            }
            Drift::SchemaChanged { tag, version, additive: true } => write!(
                f,
                "{tag} v{version}: the schema changed, and the frozen example still reads to the same bytes \
                 (an added optional field, D-167). Run `just schema-lock` to record it in the same version."
            ),
            Drift::SchemaChanged { tag, version, additive: false } => {
                write!(f, "{tag} v{version}: the serde type changed in a way the frozen example shows. {BUMP}")
            }
            Drift::Unreadable { tag, version, error } => {
                write!(f, "{tag} v{version}: the frozen example does not read as the current type ({error}). {BUMP}")
            }
            Drift::UnknownFields { tag, version, names } => write!(
                f,
                "{tag} v{version}: the current type does not know the frozen example's fields {names:?} \
                 (a field was removed or renamed). {BUMP}"
            ),
            Drift::HashMoved { tag, version, was, now } => write!(
                f,
                "{tag} v{version}: the frozen example writes other bytes ({was} is now {now}), \
                 so every document of the kind changes its hash. {BUMP}"
            ),
            Drift::Edited { tag, version, file } => write!(
                f,
                "{tag} v{version}: locks/{file} does not match its index. Never edit a lock by hand: \
                 restore the file from git."
            ),
            Drift::MigrationBroken { tag, from, error } => {
                write!(f, "{tag} v{from}: the frozen example does not migrate to the current version ({error}).")
            }
            Drift::LockAhead { tag, version, current } => write!(
                f,
                "{tag} v{version} is frozen, and this build writes v{current}. A frozen version is never removed."
            ),
            Drift::NoExample { tag, version } => {
                write!(f, "{tag} v{version}: the build has no worked example to freeze.")
            }
            Drift::ReferenceChanged { tag, version, path, was, now } => {
                let now = now.as_deref().map_or("no target".to_string(), |t| format!("{t:?}"));
                write!(f, "{tag} v{version}: {path} referred to {was:?}, and now has {now}. A changed target changes what the field means (D-232). {BUMP}")
            }
        }
    }
}

/// Every way `kind` differs from `lock`. An empty list is a pass.
pub fn check(kind: &Kind, lock: &Lock) -> Vec<Drift> {
    let tag = kind.tag.to_string();
    let mut drifts = Vec::new();
    for (&version, locked) in &lock.versions {
        drifts.extend(edited(&tag, version, locked));
        if version > kind.current {
            drifts.push(Drift::LockAhead { tag: tag.clone(), version, current: kind.current });
        } else if version < kind.current {
            if let Err(error) = migrates(kind, version, locked) {
                drifts.push(Drift::MigrationBroken { tag: tag.clone(), from: version, error });
            }
        }
    }
    let Some(locked) = lock.versions.get(&kind.current) else {
        drifts.push(Drift::Unlocked { tag, version: kind.current });
        return drifts;
    };
    let version = kind.current;
    let mut same_bytes = false;
    match serde_json::from_str::<Value>(&locked.example).map_err(|e| e.to_string()).and_then(|doc| (kind.reparse)(&doc)) {
        Err(error) => drifts.push(Drift::Unreadable { tag: tag.clone(), version, error }),
        Ok((bytes, unknown)) => {
            let now = cap_abi::content_hash(bytes.as_bytes());
            same_bytes = now == locked.example_hash && unknown.is_empty();
            if !unknown.is_empty() {
                drifts.push(Drift::UnknownFields { tag: tag.clone(), version, names: unknown });
            }
            if now != locked.example_hash {
                drifts.push(Drift::HashMoved { tag: tag.clone(), version, was: locked.example_hash.clone(), now });
            }
        }
    }
    drifts.extend(references_changed(&tag, version, &locked.schema, &kind.schema));
    if kind.schema != locked.schema {
        drifts.push(Drift::SchemaChanged { tag, version, additive: same_bytes });
    }
    drifts
}

/// Each reference the locked schema declares that the build's schema
/// declares with another target, or not at all.
fn references_changed(tag: &str, version: u32, locked: &Value, build: &Value) -> Vec<Drift> {
    let now = transit_version::references(build);
    transit_version::references(locked)
        .into_iter()
        .filter_map(|was| {
            let target = now.iter().find(|r| r.path == was.path).map(|r| r.target.clone());
            (target.as_deref() != Some(was.target.as_str())).then(|| Drift::ReferenceChanged {
                tag: tag.into(),
                version,
                path: was.path,
                was: was.target,
                now: target,
            })
        })
        .collect()
}

/// The lock that records `kind`: `lock` with the current version frozen
/// when it is new, and its schema replaced when the change is additive.
/// Any other drift is refused, and the lock is unchanged.
pub fn freeze(kind: &Kind, lock: &Lock) -> Result<Lock, Vec<Drift>> {
    let drifts = check(kind, lock);
    let refused: Vec<Drift> = drifts
        .iter()
        .filter(|d| !matches!(d, Drift::Unlocked { .. } | Drift::SchemaChanged { additive: true, .. }))
        .cloned()
        .collect();
    if !refused.is_empty() {
        return Err(refused);
    }
    let mut next = lock.clone();
    let version = kind.current;
    let tag = kind.tag.to_string();
    if let Some(locked) = next.versions.get_mut(&version) {
        *locked = Locked::new(kind.schema.clone(), locked.example.clone());
        return Ok(next);
    }
    let Some(example) = &kind.example else {
        return Err(vec![Drift::NoExample { tag, version }]);
    };
    match (kind.reparse)(example) {
        Ok((bytes, unknown)) if unknown.is_empty() => {
            next.versions.insert(version, Locked::new(kind.schema.clone(), bytes));
            Ok(next)
        }
        Ok((_, names)) => Err(vec![Drift::UnknownFields { tag, version, names }]),
        Err(error) => Err(vec![Drift::Unreadable { tag, version, error }]),
    }
}

/// Bring an older version's frozen example to the current version, and
/// read it there.
fn migrates(kind: &Kind, from: u32, locked: &Locked) -> Result<(), String> {
    let migrate = kind.migrate.ok_or_else(|| format!("the kind has no migration from v{from}"))?;
    let doc = serde_json::from_str::<Value>(&locked.example).map_err(|e| e.to_string())?;
    let (_, unknown) = (kind.reparse)(&migrate(doc, from)?)?;
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(format!("the migrated document has fields the current type does not know: {unknown:?}"))
    }
}

/// The lock files that do not match their index, or an example that is
/// not in canonical form.
fn edited(tag: &str, version: u32, locked: &Locked) -> Vec<Drift> {
    let mut out = Vec::new();
    if cap_abi::content_hash(schema_text(&locked.schema).as_bytes()) != locked.schema_hash {
        out.push(Drift::Edited { tag: tag.into(), version, file: schema_file(tag, version) });
    }
    let canonical = serde_json::from_str::<Value>(&locked.example)
        .ok()
        .and_then(|doc| cap_abi::canonical_bytes(&doc).ok())
        .is_some_and(|bytes| bytes == locked.example);
    if !canonical || cap_abi::content_hash(locked.example.as_bytes()) != locked.example_hash {
        out.push(Drift::Edited { tag: tag.into(), version, file: example_file(tag, version) });
    }
    out
}

/// A schema file's text: pretty JSON with a final newline.
fn schema_text(schema: &Value) -> String {
    serde_json::to_string_pretty(schema).expect("a schema serializes") + "\n"
}

pub fn schema_file(tag: &str, version: u32) -> String {
    format!("{tag}.v{version}.schema.json")
}

pub fn example_file(tag: &str, version: u32) -> String {
    format!("{tag}.v{version}.example.json")
}

fn index_file(tag: &str) -> String {
    format!("{tag}.lock.json")
}

/// Read a kind's lock from a `locks/` directory. A kind with no index
/// has an empty lock.
pub fn read_lock(dir: &Path, tag: &str) -> Result<Lock, String> {
    let mut lock = Lock::empty(tag);
    let index = match std::fs::read_to_string(dir.join(index_file(tag))) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(lock),
        Err(e) => return Err(format!("{}: {e}", dir.join(index_file(tag)).display())),
    };
    let index: Value = serde_json::from_str(&index).map_err(|e| format!("{}: {e}", index_file(tag)))?;
    let versions = index["versions"].as_object().ok_or_else(|| format!("{}: no versions", index_file(tag)))?;
    for (version, hashes) in versions {
        let version: u32 = version.parse().map_err(|_| format!("{}: version {version:?}", index_file(tag)))?;
        let read = |file: String| std::fs::read_to_string(dir.join(&file)).map_err(|e| format!("{file}: {e}"));
        let schema = serde_json::from_str(&read(schema_file(tag, version))?).map_err(|e| format!("{}: {e}", schema_file(tag, version)))?;
        let hash = |name: &str| hashes[name].as_str().map(str::to_string).ok_or_else(|| format!("{}: v{version} has no {name}", index_file(tag)));
        lock.versions.insert(
            version,
            Locked { schema, example: read(example_file(tag, version))?, schema_hash: hash("schema_hash")?, example_hash: hash("example_hash")? },
        );
    }
    Ok(lock)
}

/// Write a kind's lock into a `locks/` directory: every version's files,
/// then the index.
pub fn write_lock(dir: &Path, lock: &Lock) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let write = |file: String, text: &str| std::fs::write(dir.join(&file), text).map_err(|e| format!("{file}: {e}"));
    let mut versions = serde_json::Map::new();
    for (&version, locked) in &lock.versions {
        write(schema_file(&lock.tag, version), &schema_text(&locked.schema))?;
        write(example_file(&lock.tag, version), &locked.example)?;
        versions.insert(version.to_string(), json!({ "schema_hash": locked.schema_hash, "example_hash": locked.example_hash }));
    }
    let index = json!({ "tag": lock.tag, "versions": versions });
    write(index_file(&lock.tag), &(serde_json::to_string_pretty(&index).expect("an index serializes") + "\n"))
}
