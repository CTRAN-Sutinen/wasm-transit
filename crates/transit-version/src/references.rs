//! The references a kind declares (D-232): each field that names another
//! record or thing states its target in the kind's JSON Schema, with the
//! keyword `x-references`. [`references`] reads them from a schema.
//!
//! A field declares its target with schemars:
//!
//! ```ignore
//! /// The stop, by the stops document's lasting id.
//! #[schemars(extend("x-references" = "stop"))]
//! pub stop: String,
//! ```
//!
//! A target is one of these:
//!
//! - a log record kind, as the kind table names it: `stop`,
//!   `stop_request`, `service_alert`, `edit_record`, `note`;
//! - a thing of the network, by its model id: `network.route`,
//!   `network.stop`, `network.trip`;
//! - a trip instance (`trip_instance`): an object with the trip, its
//!   service date, and its start;
//! - a document by its content hash: its kind's name in the kind list
//!   (`network`, `streets`, `feed`; `crates/cap-abi/src/kinds.rs`);
//! - an attachment by its content hash (`attachment`);
//! - an entry of a log by its seq (`log.<name>`, such as `log.stops`);
//! - a thing outside the record (`external.<name>`). It is declared, and
//!   never checked.
//!
//! The kind table carries each kind's references (`references`), and
//! the schema lock compares them (a changed target needs a new version).
//! A law in each owning crate lists its reference fields and checks that
//! each is declared.

use serde_json::Value;

/// The schema keyword that declares a field's target.
pub const KEYWORD: &str = "x-references";

/// One declared reference: where it is in a document, and its target.
///
/// The path is a JSON Pointer with two additions. `*` is each item of an
/// array, or each value of an object keyed by id. `[tag=value]` after a
/// segment chooses the branch of a tagged enum, by the value of its tag
/// field (`/notes/*/target[kind=edit]/id`). A declaration on an array
/// applies to each item, so its path ends in `*`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Reference {
    pub path: String,
    pub target: String,
}

impl Reference {
    pub fn new(path: &str, target: &str) -> Self {
        Reference { path: path.into(), target: target.into() }
    }
}

/// Every reference a schema declares, in path order. A `$ref` resolves in
/// the schema's `$defs`. A definition that refers to itself is read once
/// on each path.
pub fn references(schema: &Value) -> Vec<Reference> {
    let mut out = Vec::new();
    let mut stack = Vec::new();
    walk(schema, schema, String::new(), &mut stack, &mut out);
    out.sort();
    out.dedup();
    out
}

fn walk<'a>(root: &'a Value, node: &'a Value, path: String, stack: &mut Vec<&'a str>, out: &mut Vec<Reference>) {
    let Some(obj) = node.as_object() else { return };
    if let Some(target) = obj.get(KEYWORD).and_then(Value::as_str) {
        let path = if is_array(node) { format!("{path}/*") } else { path.clone() };
        out.push(Reference { path, target: target.into() });
    }
    if let Some(name) = obj.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix("#/$defs/")) {
        if !stack.contains(&name) {
            if let Some(def) = root.get("$defs").and_then(|d| d.get(name)) {
                stack.push(name);
                walk(root, def, path.clone(), stack, out);
                stack.pop();
            }
        }
    }
    if let Some(props) = obj.get("properties").and_then(Value::as_object) {
        for (name, prop) in props {
            walk(root, prop, format!("{path}/{name}"), stack, out);
        }
    }
    for key in ["items", "additionalProperties"] {
        if let Some(inner) = obj.get(key).filter(|v| v.is_object()) {
            walk(root, inner, format!("{path}/*"), stack, out);
        }
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        for branch in obj.get(key).and_then(Value::as_array).into_iter().flatten() {
            let path = match tag_of(root, branch) {
                Some((field, value)) => format!("{path}[{field}={value}]"),
                None => path.clone(),
            };
            walk(root, branch, path, stack, out);
        }
    }
}

/// Whether a schema is an array's.
fn is_array(node: &Value) -> bool {
    match node.get("type") {
        Some(Value::String(t)) => t == "array",
        Some(Value::Array(types)) => types.iter().any(|t| t == "array"),
        _ => false,
    }
}

/// A tagged enum branch's tag: the property whose schema is a `const`
/// string.
fn tag_of(root: &Value, branch: &Value) -> Option<(String, String)> {
    let branch = match branch.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix("#/$defs/")) {
        Some(name) => root.get("$defs")?.get(name)?,
        None => branch,
    };
    branch.get("properties")?.as_object()?.iter().find_map(|(field, prop)| {
        let value = prop.get("const")?.as_str()?;
        Some((field.clone(), value.to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_declaration_is_found_through_refs_maps_arrays_and_tagged_branches() {
        let schema = json!({
            "type": "object",
            "properties": {
                "kind": {"const": "doc"},
                "items": {"type": "object", "additionalProperties": {"$ref": "#/$defs/Item"}}
            },
            "$defs": {
                "Item": {
                    "type": "object",
                    "properties": {
                        "stop": {"type": "string", "x-references": "stop"},
                        "routes": {"type": "array", "items": {"type": "string"}, "x-references": "network.route"},
                        "order": {"anyOf": [{"$ref": "#/$defs/Order"}, {"type": "null"}]},
                        "target": {"oneOf": [
                            {"type": "object", "properties": {"kind": {"type": "string", "const": "edit"}, "id": {"type": "string", "x-references": "edit_record"}}},
                            {"type": "object", "x-references": "trip_instance", "properties": {"kind": {"type": "string", "const": "trip"}, "trip": {"type": "string"}}}
                        ]},
                        "next": {"$ref": "#/$defs/Item"}
                    }
                },
                "Order": {"type": "object", "properties": {"id": {"type": "string", "x-references": "external.work_order"}}}
            }
        });
        assert_eq!(
            references(&schema),
            vec![
                Reference::new("/items/*/order/id", "external.work_order"),
                Reference::new("/items/*/routes/*", "network.route"),
                Reference::new("/items/*/stop", "stop"),
                Reference::new("/items/*/target[kind=edit]/id", "edit_record"),
                Reference::new("/items/*/target[kind=trip]", "trip_instance"),
            ]
        );
    }

    #[test]
    fn a_schema_with_no_declaration_has_no_references() {
        assert!(references(&json!({"type": "object", "properties": {"a": {"type": "string"}}})).is_empty());
    }
}
