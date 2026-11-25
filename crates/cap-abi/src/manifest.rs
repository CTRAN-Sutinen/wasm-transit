//! The TS bindings of these types are generated into `types/cap_abi/`.
//!
//! If a document is at version V and a module speaks W, and a migration path
//! V to W exists, the assembler migrates the document. The migration is
//! immutable, and the old bytes stay addressable. If no path exists, the
//! module is unavailable for that document, and the assembly degrades for
//! each capability.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct CapManifest {
    /// A module rejects a document with another version at load, not later
    /// inside an op.
    pub kernel: u32,
    #[serde(default)]
    pub kernel_ops: Vec<KernelOp>,
    pub capabilities: Vec<Capability>,
    /// Each capability above is also built alone, at its `url`, for runners,
    /// agents, and the server.
    #[serde(default)]
    pub assemblies: Vec<LinkedAssembly>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct LinkedAssembly {
    pub ui: String,
    /// "sha256:<hex>".
    pub hash: String,
    /// Contains the hash.
    pub url: String,
    pub bytes: u64,
    /// In index order, because a bulk op carries the capability's index in
    /// the high 16 bits.
    pub capabilities: Vec<String>,
}

/// The request of its worked example is the bytes, as JSON.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct KernelOp {
    pub code: u32,
    pub name: String,
    /// Absent for an op that is not a load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<String>,
    pub description: String,
    pub example: Option<OpExample>,
}

/// The host verifies the fetched bytes against the hash.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct Capability {
    pub id: String,
    /// "sha256:<hex>".
    pub hash: String,
    /// Contains the hash.
    pub url: String,
    pub bytes: u64,
    /// The kernel loads are not among them, because they are `kernel_ops`.
    pub ops: Vec<String>,
    /// Every module that links the capability answers the kernel loads of
    /// these kinds.
    #[serde(default)]
    pub documents: Vec<String>,
    /// Each kind maps to the id of the capability that answers it.
    pub consumes: BTreeMap<String, String>,
    pub speaks: BTreeMap<String, Vec<u32>>,
    /// Each step is a pure, deterministic op over document bytes. The old
    /// copy stays addressable, because migrations are immutable. A rollback
    /// moves the current pointer back along `produced_by`.
    pub migrates: BTreeMap<String, Vec<MigrationSpec>>,
    pub purpose: String,
    pub op_docs: BTreeMap<String, OpDoc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct MigrationSpec {
    pub from: u32,
    pub to: u32,
}

/// The examples are raw JSON, so the float_roundtrip workspace feature keeps
/// them exact.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct OpDoc {
    pub description: String,
    pub example: Option<OpExample>,
    /// One example for each tag of an op whose argument is a tagged
    /// vocabulary, such as the edits of cap-author `apply`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<KindExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct KindExample {
    pub kind: String,
    #[ts(type = "unknown")]
    pub example: serde_json::Value,
}

/// The generator runs `loads`, then `request`, through the capability's
/// `invoke` and records the answer, so an example cannot differ from the
/// dispatch. A bulk op gives `bytes`, which the generator runs through
/// `invoke_bytes`, and `request` describes the bytes for a reader.
#[derive(Debug, Clone)]
pub struct Example {
    pub bytes: Option<(u32, Vec<u8>)>,
    pub kinds: Vec<KindExample>,
    pub op: &'static str,
    pub loads: Vec<serde_json::Value>,
    pub request: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct OpExample {
    /// TS sees only `unknown`.
    #[ts(type = "unknown")]
    pub request: serde_json::Value,
    #[ts(type = "unknown")]
    pub response: serde_json::Value,
}
