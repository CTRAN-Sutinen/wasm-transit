//! The real-output gate and the soak share this check ([`crate::checks`]).
//! A document is an object whose `kind` is a locked tag and that carries a
//! numeric `schema_version`. A document nested in another one, and a
//! document that an op answers as its exact bytes (a string holding a JSON
//! object, such as the `document` of `plans.publish`), are checked on their
//! own.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

pub struct Locks(BTreeMap<String, jsonschema::Validator>);

impl Locks {
    pub fn read() -> Self {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut out = BTreeMap::new();
        for owner in std::fs::read_dir(&crates).expect("read crates/").flatten() {
            let locks = owner.path().join("locks");
            let Ok(files) = std::fs::read_dir(&locks) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name().to_string_lossy().to_string();
                let Some(tag) = name.strip_suffix(".lock.json") else {
                    continue;
                };
                let index: Value =
                    serde_json::from_str(&std::fs::read_to_string(file.path()).unwrap()).unwrap();
                for version in index["versions"]
                    .as_object()
                    .expect("a lock lists its versions")
                    .keys()
                {
                    let path = locks.join(format!("{tag}.v{version}.schema.json"));
                    let schema: Value =
                        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                    // schemars writes formats (uint32, double) that are no
                    // standard format, so formats are not asserted.
                    let validator = jsonschema::options()
                        .should_validate_formats(false)
                        .build(&schema)
                        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                    out.insert(format!("{tag} v{version}"), validator);
                }
            }
        }
        assert!(
            !out.is_empty(),
            "no locks under crates/*/locks/ (run `just schema-lock`)"
        );
        Locks(out)
    }

    fn tags(&self) -> Vec<&str> {
        self.0
            .keys()
            .map(|key| key.split(' ').next().unwrap())
            .collect()
    }

    pub fn check(&self, answer: &Value) -> Vec<(String, Vec<String>)> {
        let tags = self.tags();
        let mut found = Vec::new();
        documents(answer, &tags, &mut found);
        found
            .iter()
            .map(|doc| {
                let key = format!(
                    "{} v{}",
                    doc["kind"].as_str().unwrap(),
                    doc["schema_version"]
                );
                let problems = match self.0.get(&key) {
                    None => vec!["the locks have no schema for that version".to_string()],
                    Some(validator) => validator
                        .iter_errors(doc)
                        .take(3)
                        .map(|e| format!("{}: {e}", e.instance_path))
                        .collect(),
                };
                (key, problems)
            })
            .collect()
    }
}

fn documents(value: &Value, tags: &[&str], out: &mut Vec<Value>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| documents(item, tags, out)),
        Value::Object(fields) => {
            let tagged = fields
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|k| tags.contains(&k));
            if tagged && fields.get("schema_version").is_some_and(Value::is_u64) {
                out.push(value.clone());
            }
            fields
                .values()
                .for_each(|child| documents(child, tags, out));
        }
        Value::String(text) if text.starts_with('{') => {
            if let Ok(inner @ Value::Object(_)) = serde_json::from_str::<Value>(text) {
                documents(&inner, tags, out);
            }
        }
        _ => {}
    }
}
