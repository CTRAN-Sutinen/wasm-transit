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

    #[test]
    fn the_schema_is_the_constant() {
        let schema = schemars::schema_for!(ThingKind);
        assert_eq!(schema.get("const").and_then(|v| v.as_str()), Some("thing"));
    }
}
