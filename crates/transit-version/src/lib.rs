//! What a document says about itself (D-62, D-115): its kind and its
//! version, inside it, so a copy anywhere — a blob, an export, a file
//! another tool reads — names both without the app
//! (docs/FORMATS.md, "Versions").
//!
//! - [`kind_tag!`] declares a kind's tag: a field that always writes
//!   the kind's name, reads an absent one as itself, and refuses a
//!   document naming another kind.
//! - [`Version<N>`] is a version field whose absence reads as `N` —
//!   for a kind whose untagged copies are that version.
//! - [`Versioned`] is what a reader needs to refuse another version
//!   at its load, naming both (`cap_abi::parse_versioned`).
//! - [`Extra`] is a record's fields that this build does not know,
//!   kept as written (D-167).

#![forbid(unsafe_code)]

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
