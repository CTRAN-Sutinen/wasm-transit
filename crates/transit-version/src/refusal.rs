//! A host acts on a refusal's `code`, never on its `message`, which is text for
//! a person (docs/architecture/bus.md, "Answers"). There is no conversion back
//! to `String`, so a layer that would drop the code does not compile.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refusal {
    pub message: String,
    #[serde(flatten)]
    pub code: Option<RefusalCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "code", rename_all = "snake_case")]
#[ts(export, export_to = "transit_version/")]
pub enum RefusalCode {
    /// The module does not hold the document. The host registers it with
    /// `load` and asks again.
    UnknownDocument { kind: String, hash: String, load: String },
    /// A newer build wrote the document. `read` is true when this build read
    /// it one version ahead and refused only a write over it.
    NewerVersion { kind: String, found: u32, reads: u32, read: bool },
}

impl Refusal {
    pub fn unknown_document(kind: &str, hash: &str, load: &str) -> Self {
        Refusal {
            message: format!("unknown {kind} {hash}: {load} first"),
            code: Some(RefusalCode::UnknownDocument { kind: kind.into(), hash: hash.into(), load: load.into() }),
        }
    }

    pub fn context(mut self, place: impl std::fmt::Display) -> Self {
        self.message = format!("{place}: {}", self.message);
        self
    }
}

impl From<String> for Refusal {
    fn from(message: String) -> Self {
        Refusal { message, code: None }
    }
}

impl From<&str> for Refusal {
    fn from(message: &str) -> Self {
        Refusal { message: message.to_string(), code: None }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
