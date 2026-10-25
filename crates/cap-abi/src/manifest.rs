//! The capability manifest: the assembly itself. A deployment is a
//! manifest; "ship capabilities to match the user" is a statement
//! about this document.
//!
//! The types are the contract three things hang off — the browser
//! resolver, the (future) Go assembler, and the corpus/provenance
//! tooling — so they live here, next to the ABI, with TS bindings
//! generated into `types/cap_abi/`.
//!
//! Two parts that only matter together:
//!
//! * **Compatibility** — `speaks` (which document versions a module
//!   understands directly) and `migrates` (which version steps it can
//!   bridge). The resolver's version arithmetic is: local document at
//!   version V, module speaking W, migration path V→…→W available →
//!   migrate (immutably — the old bytes stay addressable), else the
//!   module is unavailable FOR THAT DOCUMENT and the assembly
//!   degrades per capability.
//! * **Agent context** — `purpose` and `op_docs` are the discovery
//!   surface for agents that assemble apps from a registry: what the
//!   capability is FOR, what each op does, and a worked
//!   request→response example per op (the corpus-as-examples idea).
//!   Context lives IN the manifest so it versions with the capability
//!   and its diff is reviewable like any other contract change.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The document-kernel version this build speaks. A module rejects a
/// document whose version is not its own AT LOAD (the seam), never
/// three layers deep in an op.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct CapManifest {
    pub kernel: u32,
    pub capabilities: Vec<Capability>,
}

/// One capability module: what it is, the exact bytes it is (hash-
/// pinned; the host verifies fetched bytes against this), what it
/// answers, what it can read, and what it can bridge.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct Capability {
    /// The identity screens declare needs by ("router", "gtfs", ...).
    pub id: String,
    /// "sha256:<hex>" of the module bytes — integrity at the seam.
    pub hash: String,
    /// Where the module bytes are served (hash-addressed, so the
    /// HTTP cache holds each build forever).
    pub url: String,
    /// Module size in bytes — the size budget watches this.
    pub bytes: u64,
    /// The JSON ops this module's dispatch answers.
    pub ops: Vec<String>,
    /// Registry edges: document kind -> producing capability id.
    pub consumes: HashMap<String, String>,
    /// Document kinds -> versions the module understands directly.
    pub speaks: HashMap<String, Vec<u32>>,
    /// Document kinds -> version steps the module can bridge. Each
    /// step is a pure, deterministic op over document bytes: the old
    /// copy stays addressable (migrations are immutable; rollback is
    /// moving the current pointer back along `produced_by`).
    pub migrates: HashMap<String, Vec<MigrationSpec>>,
    /// Agent context, versioned with the capability: what this
    /// module is for and when to assemble it.
    pub purpose: String,
    /// Per-op agent context: description plus a worked example.
    pub op_docs: HashMap<String, OpDoc>,
}

/// One migration step a module can perform.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct MigrationSpec {
    pub from: u32,
    pub to: u32,
}

/// Agent-facing documentation for one op: what it does, plus a worked
/// request→response pair an agent can pattern-match against. Examples
/// are raw JSON, so the float_roundtrip workspace feature keeps them
/// exact.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct OpDoc {
    pub description: String,
    pub example: Option<OpExample>,
}

/// A worked request→response pair.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_abi/")]
pub struct OpExample {
    /// Raw JSON — the agent reads the shape, TS just sees `unknown`.
    #[ts(type = "unknown")]
    pub request: serde_json::Value,
    #[ts(type = "unknown")]
    pub response: serde_json::Value,
}
