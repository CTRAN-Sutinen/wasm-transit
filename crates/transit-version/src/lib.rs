//! A document names its kind and version inside it (D-62, D-115), so any copy
//! says both without the app. The kinds and their versions are in
//! docs/FORMATS.md, "Versions".

#![forbid(unsafe_code)]

pub mod document;
pub mod jcs;
pub mod load;
pub mod references;
pub mod refusal;
pub use document::{canonical_hash, parse_versioned};
pub use load::{Form, Load, Retain};
pub use references::{references, Reference};
pub use refusal::{Refusal, RefusalCode};

#[doc(hidden)]
pub use schemars;
#[doc(hidden)]
pub use serde;

pub trait Versioned {
    const KIND: &'static str;
    const READS: u32;
    const VERSION_FIELD: &'static str = "schema_version";
    const UNTAGGED: u32 = 1;
    fn schema_version(&self) -> u32;
    /// Reads the version from JSON that does not read as this type, so the
    /// refusal can name it. A kind whose older copies name their version
    /// another way overrides this.
    fn version_in(value: &serde_json::Value) -> u32 {
        value.get(Self::VERSION_FIELD).and_then(serde_json::Value::as_u64).map_or(Self::UNTAGGED, |v| v as u32)
    }
    /// A format change ships in two releases (D-79, D-186). Release A reads
    /// vN and vN+1 and writes vN, and release B writes vN+1. So a rollback
    /// from B to A reads what B stored.
    const AHEAD: Option<(u32, Ahead)> = None;
    /// The step lives in the crate that owns the kind, beside the migration
    /// step that it reverses.
    fn step_down(next: serde_json::Value) -> Result<serde_json::Value, String> {
        let _ = next;
        Err(format!("{} has no step down from a newer version", Self::KIND))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ahead {
    /// vN+1 read down and brought forward again is the same document, so
    /// writes over it proceed.
    Lossless,
    /// vN+1 holds what vN cannot, so a write over it is refused. A write's
    /// base reads through `cap_abi::Base` or `Kind::require_writable`
    /// (docs/FORMATS.md, "Rolling a version back").
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadAs {
    Current,
    Ahead(Ahead),
}

impl ReadAs {
    pub fn read_only(self) -> bool {
        self == ReadAs::Ahead(Ahead::ReadOnly)
    }
}

pub fn reads<T: Versioned>(found: u32) -> Result<ReadAs, Refusal> {
    match T::AHEAD {
        _ if found == T::READS => Ok(ReadAs::Current),
        Some((next, how)) if found == next => Ok(ReadAs::Ahead(how)),
        _ => Err(refused(T::KIND, found, T::READS)),
    }
}

/// An older copy names the op that migrates it. A newer copy is refused and
/// never rewritten.
pub fn refused(kind: &str, found: u32, reads: u32) -> Refusal {
    if found < reads {
        format!("{kind} v{found}: this module reads v{reads} — bring it forward first (migrate.migrate, kind {kind:?})").into()
    } else {
        Refusal {
            message: format!("{kind} v{found} is newer than this module, which reads v{reads}: a newer build wrote it (refused, never rewritten)"),
            code: Some(RefusalCode::NewerVersion { kind: kind.into(), found, reads, read: false }),
        }
    }
}

pub fn write_refused(kind: &str, found: u32, reads: u32) -> Refusal {
    Refusal {
        message: format!(
            "{kind} v{found} was written by a newer build: this build reads it one version ahead of v{reads}, \
             and does not write over it — move the deployment forward again to change it (D-186)"
        ),
        code: Some(RefusalCode::NewerVersion { kind: kind.into(), found, reads, read: true }),
    }
}

pub fn writes_over<T: Versioned>(found: u32, read: ReadAs) -> Result<(), Refusal> {
    if read.read_only() {
        Err(write_refused(T::KIND, found, T::READS))
    } else {
        Ok(())
    }
}

/// The fields of a record that this build does not know (D-167). They stay
/// as written through a read and a write, so a writer on an older build does
/// not drop what a newer one wrote. Every record of an authored kind carries
/// one, flattened into the record. A value (a point, a set of weekdays)
/// carries none. The write path flags a field kept here for a person and
/// never refuses it.
///
/// Equality, order, and hashing read the fields' JSON text, so a record that
/// derives them may carry one.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct Extra(pub std::collections::BTreeMap<String, serde_json::Value>);

thread_local! {
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

/// Returns the answer beside the names of the unknown fields that `read`
/// met (D-167). A read outside it notes nothing.
pub fn noting_unknown<R>(read: impl FnOnce() -> R) -> (R, Vec<String>) {
    let outer = NOTED.with(|noted| noted.borrow_mut().replace(Default::default()));
    let answer = read();
    let names = NOTED.with(|noted| {
        let mine = noted.borrow_mut().take().unwrap_or_default();
        // A nested reading is noted in both.
        if let Some(mut outer) = outer {
            outer.extend(mine.iter().cloned());
            *noted.borrow_mut() = Some(outer);
        }
        mine
    });
    (answer, names.into_iter().collect())
}

impl Extra {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

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

/// Declares a kind's tag, as in `kind_tag!(PlansKind = "plans");`. The field
/// needs `#[serde(default)]` for an absent tag to read as the kind.
#[macro_export]
macro_rules! kind_tag {
    ($(#[$meta:meta])* $name:ident = $kind:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        #[allow(dead_code)]
        impl $name {
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
        let (_, none) = super::noting_unknown(|| ());
        assert!(none.is_empty());
    }

    #[test]
    fn the_schema_is_the_constant() {
        let schema = schemars::schema_for!(ThingKind);
        assert_eq!(schema.get("const").and_then(|v| v.as_str()), Some("thing"));
    }
}
