//! What a document says about itself (D-62, D-115): its kind and version,
//! inside it, so any copy names both without the app. The kinds and their
//! versions are in docs/FORMATS.md, "Versions".
//!
//! - [`kind_tag!`] declares a kind's tag: always written; an absent one
//!   reads as the kind; another kind is refused.
//! - [`Version<N>`] is a version field whose absence reads as `N`.
//! - [`Versioned`] refuses another version at load, naming both
//!   ([`parse_versioned`]). For one release a kind may read the next
//!   version ([`Ahead`], D-79, D-186): the owning crate's `step_down` is
//!   lossless and writable, or else the copy is read-only and a write over
//!   it is refused. A write's base reads through `cap_abi::Base` or
//!   `Kind::require_writable` (docs/FORMATS.md, "Rolling a version back").
//! - [`Extra`] keeps a record's unknown fields through a read and a write
//!   (D-167). Values (a point, a time pair) carry none. Every load and
//!   receive op names the unknown fields it met.
//! - [`references`] reads the references a schema declares
//!   (`x-references`, D-232).
//! - [`Load`] is how the kernel loads a kind; each library implements it
//!   in its `src/load.rs` (D-239).
//! - [`document`] is identity and read: the canonical hash (RFC 8785) and
//!   the versioned read of JSON or gzipped JSON.

#![forbid(unsafe_code)]

pub mod document;
pub mod jcs;
pub mod load;
pub mod references;
pub use document::{canonical_hash, parse_versioned};
pub use load::{Form, Load, Retain};
pub use references::{references, Reference};

#[doc(hidden)]
pub use schemars;
#[doc(hidden)]
pub use serde;

/// A document kind a reader checks the version of: its name, the one
/// version this build reads, where the version lives, and the version
/// a copy carries.
pub trait Versioned {
    /// The kind's name, as its tag writes it.
    const KIND: &'static str;
    /// The version this build reads and writes.
    const READS: u32;
    /// The field the version lives in.
    const VERSION_FIELD: &'static str = "schema_version";
    /// What a copy with no version field is.
    const UNTAGGED: u32 = 1;
    /// The version this copy carries.
    fn schema_version(&self) -> u32;
    /// The version a copy carries, read from its JSON when it does not
    /// read as this type (`cap_abi::parse_versioned` names it in the
    /// refusal): the number in [`Self::VERSION_FIELD`], else
    /// [`Self::UNTAGGED`]. A kind whose older copies name their version
    /// another way reads that too.
    fn version_in(value: &serde_json::Value) -> u32 {
        value.get(Self::VERSION_FIELD).and_then(serde_json::Value::as_u64).map_or(Self::UNTAGGED, |v| v as u32)
    }
    /// The next version this build also reads, for one release, and how
    /// (D-79, D-186). A format change ships in two releases: release A
    /// reads vN and vN+1 and writes vN, and release B writes vN+1. So a
    /// rollback from B to A reads what B stored. `None`: this build
    /// reads only [`Self::READS`].
    const AHEAD: Option<(u32, Ahead)> = None;
    /// A copy in the version [`Self::AHEAD`] names, stepped down to the
    /// version this build reads. It is called only for that version.
    /// The step lives in the crate that owns the kind, beside the
    /// migration step it reverses.
    fn step_down(next: serde_json::Value) -> Result<serde_json::Value, String> {
        let _ = next;
        Err(format!("{} has no step down from a newer version", Self::KIND))
    }
}

/// How a build reads the next version of a kind (D-186). The format
/// change declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ahead {
    /// The step down loses nothing: vN+1 read down and brought forward
    /// again is the same document (a law pins it). The build reads the
    /// document and writes over it as usual.
    Lossless,
    /// vN+1 holds what vN cannot. The build reads the document through
    /// the step down, and refuses to write over it, naming both
    /// versions, until the deployment moves forward again.
    ReadOnly,
}

/// How a copy was read: in the version this build reads, or one
/// version ahead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadAs {
    Current,
    Ahead(Ahead),
}

impl ReadAs {
    /// Whether a write over the copy is refused: a copy read ahead
    /// through a read-only step.
    pub fn read_only(self) -> bool {
        self == ReadAs::Ahead(Ahead::ReadOnly)
    }
}

/// How this build reads a copy of `T`'s kind in version `found`: the
/// version it reads, the version [`Versioned::AHEAD`] names, or the
/// refusal naming both ([`refused`]).
pub fn reads<T: Versioned>(found: u32) -> Result<ReadAs, String> {
    match T::AHEAD {
        _ if found == T::READS => Ok(ReadAs::Current),
        Some((next, how)) if found == next => Ok(ReadAs::Ahead(how)),
        _ => Err(refused(T::KIND, found, T::READS)),
    }
}

/// The refusal of a copy in another version, naming both: an older one
/// with the capability op that migrates it, a newer one as written by a
/// newer build (refused, never rewritten).
pub fn refused(kind: &str, found: u32, reads: u32) -> String {
    if found < reads {
        format!("{kind} v{found}: this module reads v{reads} — bring it forward first (migrate.migrate, kind {kind:?})")
    } else {
        format!("{kind} v{found} is newer than this module, which reads v{reads}: a newer build wrote it (refused, never rewritten)")
    }
}

/// The refusal of a write over a copy read ahead through a read-only
/// step (D-186), naming both versions.
pub fn write_refused(kind: &str, found: u32, reads: u32) -> String {
    format!(
        "{kind} v{found} was written by a newer build: this build reads it one version ahead of v{reads}, \
         and does not write over it — move the deployment forward again to change it (D-186)"
    )
}

/// The refusal of a write over a copy of `T`'s kind read as `read`, if
/// it is refused.
pub fn writes_over<T: Versioned>(found: u32, read: ReadAs) -> Result<(), String> {
    if read.read_only() {
        Err(write_refused(T::KIND, found, T::READS))
    } else {
        Ok(())
    }
}

/// The fields of a record that this build does not know (D-167): kept
/// as written through a read and a write, so a writer on an older build
/// does not drop what a newer one wrote. Every record of an authored
/// kind carries one, flattened into the record (`#[serde(flatten)]`).
/// A value (a point, a stop's two times, a set of weekdays) is not a
/// record and carries none. The receive path flags a field kept here
/// for a person, and never refuses it.
///
/// Equality, order, and hashing read the fields' JSON text, so a record
/// that derives them may carry one.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct Extra(pub std::collections::BTreeMap<String, serde_json::Value>);

thread_local! {
    /// The names of the unknown fields read while [`noting_unknown`]
    /// runs; none is kept outside it.
    static NOTED: std::cell::RefCell<Option<std::collections::BTreeSet<String>>> = const { std::cell::RefCell::new(None) };
}

impl<'de> serde::Deserialize<'de> for Extra {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let fields = <std::collections::BTreeMap<String, serde_json::Value>>::deserialize(d)?;
        if !fields.is_empty() {
            NOTED.with(|noted| {
                if let Some(noted) = noted.borrow_mut().as_mut() {
                    noted.extend(fields.keys().cloned());
                }
            });
        }
        Ok(Extra(fields))
    }
}

/// Run `read` and answer, beside its answer, the names of the fields
/// it read that this build does not know (D-167), in name order: what
/// a receive path flags for a person. A read outside it notes nothing.
pub fn noting_unknown<R>(read: impl FnOnce() -> R) -> (R, Vec<String>) {
    let outer = NOTED.with(|noted| noted.borrow_mut().replace(Default::default()));
    let answer = read();
    let names = NOTED.with(|noted| {
        let mine = noted.borrow_mut().take().unwrap_or_default();
        // a reading nested in another one is noted in both
        if let Some(mut outer) = outer {
            outer.extend(mine.iter().cloned());
            *noted.borrow_mut() = Some(outer);
        }
        mine
    });
    (answer, names.into_iter().collect())
}

impl Extra {
    /// Whether every field is known.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The names of the fields not known, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    fn text(&self) -> String {
        serde_json::to_string(&self.0).expect("JSON values serialize")
    }
}

impl Eq for Extra {}

impl PartialOrd for Extra {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Extra {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.0.is_empty() && other.0.is_empty() {
            return std::cmp::Ordering::Equal;
        }
        self.text().cmp(&other.text())
    }
}

impl std::hash::Hash for Extra {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if !self.0.is_empty() {
            self.text().hash(state);
        }
    }
}

impl schemars::JsonSchema for Extra {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Extra".into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "object", "additionalProperties": true })
    }
}

/// A version field whose absence reads as `N`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Version<const N: u32>(pub u32);

impl<const N: u32> Default for Version<N> {
    fn default() -> Self {
        Version(N)
    }
}

impl<const N: u32> schemars::JsonSchema for Version<N> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SchemaVersion".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "integer", "minimum": 1 })
    }
}

/// Declare a kind's tag type: `kind_tag!(PlansKind = "plans");`. The
/// tag serializes as the kind's name, reads an absent field as itself
/// (with `#[serde(default)]` on the field), and refuses a document
/// that names another kind. Its JSON Schema is the constant.
#[macro_export]
macro_rules! kind_tag {
    ($(#[$meta:meta])* $name:ident = $kind:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        #[allow(dead_code)]
        impl $name {
            /// The kind's name, as the tag writes it.
            pub const KIND: &'static str = $kind;
        }

        impl $crate::serde::Serialize for $name {
            fn serialize<S: $crate::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str($kind)
            }
        }

        impl<'de> $crate::serde::Deserialize<'de> for $name {
            fn deserialize<D: $crate::serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let kind = <String as $crate::serde::Deserialize>::deserialize(d)?;
                if kind == $kind {
                    Ok($name)
                } else {
                    Err(<D::Error as $crate::serde::de::Error>::custom(format!(
                        "a {kind:?} document is not a {:?} document",
                        $kind
                    )))
                }
            }
        }

        impl $crate::schemars::JsonSchema for $name {
            fn schema_name() -> ::std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(_: &mut $crate::schemars::SchemaGenerator) -> $crate::schemars::Schema {
                $crate::schemars::json_schema!({ "const": $kind })
            }
        }
    };
}

#[cfg(test)]
mod tests {
    kind_tag!(ThingKind = "thing");

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Thing {
        #[serde(default)]
        kind: ThingKind,
        #[serde(default)]
        schema_version: super::Version<1>,
        n: u32,
    }

    #[test]
    fn the_tag_writes_its_kind_reads_an_absent_one_as_itself_and_refuses_another() {
        let thing = Thing { kind: ThingKind, schema_version: Default::default(), n: 3 };
        let text = serde_json::to_string(&thing).unwrap();
        assert_eq!(text, r#"{"kind":"thing","schema_version":1,"n":3}"#);
        assert_eq!(serde_json::from_str::<Thing>(r#"{"n":3}"#).unwrap(), thing, "absent tags read as the kind, v1");
        let err = serde_json::from_str::<Thing>(r#"{"kind":"other","n":3}"#).unwrap_err().to_string();
        assert!(err.contains(r#"a "other" document is not a "thing" document"#), "{err}");
    }

    #[derive(serde::Deserialize)]
    struct Record {
        #[allow(dead_code)]
        n: u32,
        #[serde(flatten)]
        extra: super::Extra,
        #[serde(default)]
        inner: Vec<Record>,
    }

    #[test]
    fn a_read_notes_the_fields_it_does_not_know_and_keeps_them() {
        let text = r#"{"n":1,"later":true,"inner":[{"n":2,"newer":"x"},{"n":3}]}"#;
        let (record, names) = super::noting_unknown(|| serde_json::from_str::<Record>(text).unwrap());
        assert_eq!(names, vec!["later".to_string(), "newer".to_string()]);
        assert_eq!(record.extra.names().collect::<Vec<_>>(), ["later"]);
        assert_eq!(record.inner[0].extra.names().collect::<Vec<_>>(), ["newer"]);
        assert!(record.inner[1].extra.is_empty());
        // outside it, nothing is noted
        let (_, none) = super::noting_unknown(|| ());
        assert!(none.is_empty());
    }

    #[test]
    fn the_schema_is_the_constant() {
        let schema = schemars::schema_for!(ThingKind);
        assert_eq!(schema.get("const").and_then(|v| v.as_str()), Some("thing"));
    }
}
