//! The capability manifest: the assembly itself. A deployment is a
//! manifest.
//!
//! The types are the contract the browser's bus worker, the (future) Go
//! assembler, and the registry tooling hang off. They live here, next to
//! the ABI, with TS bindings generated into `types/cap_abi/`.
//!
//! Two parts that only matter together:
//!
//! * **Compatibility**: `speaks` (which document versions a module
//!   understands directly) and `migrates` (which version steps it can
//!   bridge). The assembler's version arithmetic is: local document at
//!   version V, module speaking W, migration path V→…→W available →
//!   migrate (immutably; the old bytes stay addressable). Otherwise the
//!   module is unavailable for that document, and the assembly
//!   degrades per capability.
//! * **Agent context**: `purpose` and `op_docs` are the discovery
//!   surface for agents that assemble apps from a registry. They say
//!   what the capability is for, and what each op does. They give a
//!   worked request→response example per op. For an op whose argument
//!   is a tagged vocabulary (cap-author `apply`'s edits), they give one
//!   worked example per kind. The context versions with the capability,
//!   and its diff is reviewable like any other contract change.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The document-kernel version this build speaks. A module rejects a
/// document whose version is not its own at load (the seam), never
/// deep in an op.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct CapManifest {
    pub kernel: u32,
    /// The kernel ops: bulk ops numbered from 256 that mean the same
    /// thing in every module that answers them (the registry ops in
    /// every module), each with its worked example.
    #[serde(default)]
    pub kernel_ops: Vec<KernelOp>,
    pub capabilities: Vec<Capability>,
    /// The linked assemblies: per generated UI, one module linking the
    /// capabilities it uses (D-77), over one kernel registry. Each
    /// capability above is also built alone (its `url`), for runners,
    /// agents, and the server.
    #[serde(default)]
    pub assemblies: Vec<LinkedAssembly>,
}

/// One linked module: the capabilities it links (in index order: a
/// capability's bulk op carries its index in the high 16 bits), and
/// its exact bytes, pinned like a capability's.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct LinkedAssembly {
    /// The generated UI it is for (`web/src/uis/<ui>/`).
    pub ui: String,
    /// "sha256:<hex>" of the module bytes.
    pub hash: String,
    /// Where the bytes are served (hash-addressed).
    pub url: String,
    pub bytes: u64,
    /// The capability ids it links, in index order.
    pub capabilities: Vec<String>,
}

/// One kernel op: its bulk op code, its name, what it does, and a
/// worked request (the bytes, as JSON) → response pair.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct KernelOp {
    pub code: u32,
    pub name: String,
    /// The kind a load registers, as `capability! { documents: [...] }`
    /// names it (`crates/cap-abi/src/kinds.rs`). Absent for an op that
    /// is not a load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<String>,
    pub description: String,
    pub example: Option<OpExample>,
}

/// One capability module: what it is, the exact bytes it is (hash-
/// pinned; the host verifies fetched bytes against this), what it
/// answers, what it can read, and what it can bridge.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct Capability {
    /// The identity screens declare needs by ("router", "gtfs", ...).
    pub id: String,
    /// "sha256:<hex>" of the module bytes.
    pub hash: String,
    /// Where the module bytes are served (hash-addressed).
    pub url: String,
    /// Module size in bytes. The size budget watches this.
    pub bytes: u64,
    /// The JSON ops this capability's dispatch answers (the kernel's
    /// loads are not among them. They are `kernel_ops`).
    pub ops: Vec<String>,
    /// The kernel document kinds it reads ("network", "edits", …): the
    /// kernel loads every module linking it answers.
    #[serde(default)]
    pub documents: Vec<String>,
    /// Registry edges: document kind -> producing capability id.
    pub consumes: BTreeMap<String, String>,
    /// Document kinds -> versions the module understands directly.
    pub speaks: BTreeMap<String, Vec<u32>>,
    /// Document kinds -> version steps the module can bridge. Each
    /// step is a pure, deterministic op over document bytes: the old
    /// copy stays addressable (migrations are immutable; rollback is
    /// moving the current pointer back along `produced_by`).
    pub migrates: BTreeMap<String, Vec<MigrationSpec>>,
    /// Agent context, versioned with the capability: what this
    /// module is for and when to assemble it.
    pub purpose: String,
    /// Per-op agent context: description plus a worked example.
    pub op_docs: BTreeMap<String, OpDoc>,
}

/// One migration step a module can perform.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct MigrationSpec {
    pub from: u32,
    pub to: u32,
}

/// Agent-facing documentation for one op: what it does, plus a worked
/// request→response pair. Examples are raw JSON, so the float_roundtrip
/// workspace feature keeps them exact.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct OpDoc {
    pub description: String,
    pub example: Option<OpExample>,
    /// An op whose argument is a tagged vocabulary (cap-author
    /// `apply`'s edits): one worked example per kind, so every kind's
    /// shape is readable here, not only in the generated types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<KindExample>,
}

/// One kind of an op's tagged argument, worked: the kind's tag and a
/// value of it that the op accepts.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct KindExample {
    pub kind: String,
    #[ts(type = "unknown")]
    pub example: serde_json::Value,
}

/// A capability's worked example of one op, as its crate gives it
/// (its `examples` feature): the kernel loads to run first, each a JSON
/// load request, and the op's request. The generator runs both through
/// the capability's own `invoke` and records what it answers, so an
/// example cannot differ from dispatch. An op on the bulk lane gives its
/// code and bytes (`bytes`): the generator runs `invoke_bytes`, and
/// `request` describes the bytes for a reader. An op whose argument is a
/// tagged vocabulary gives one value of each tag (`kinds`).
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
    /// Raw JSON. The agent reads the shape; TS sees only `unknown`.
    #[ts(type = "unknown")]
    pub request: serde_json::Value,
    #[ts(type = "unknown")]
    pub response: serde_json::Value,
}
