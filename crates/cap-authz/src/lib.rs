//! cap-authz: may this identity perform this action on this pointer?
//! (OPS.md has the ops.)
//!
//! Authentication is the IdP's: the host verifies the JWT (D-60) and reads
//! the roles claim (D-59). This module owns only the decision, over the
//! policy document (kind `policy`, D-61) that the host pushes on each call.
//!
//! * The verdict is fail-closed: no policy, no subject, or no matching
//!   rule is a deny. It never depends on rule order. transit-authz holds
//!   the property tests.
//! * `validate` is the policy's check in the kind table (D-202). A policy
//!   that grants no role a write on the policy is accepted with a warning.
//! * Stateless, never cached across calls. The policy's hash is the cache
//!   key if a host wants one.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_authz/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// May `identity` perform `action` ("read" | "write") on `pointer`
    /// under `policy`? Fail-closed; the verdict never depends on rule
    /// order (transit_authz::authorize, property-tested).
    Authorize {
        identity: transit_authz::Identity,
        action: String,
        pointer: String,
        #[ts(as = "transit_authz::Policy")]
        policy: cap_abi::Current<transit_authz::Policy>,
    },
    /// The policy AS IT APPLIES TO THIS IDENTITY — the client's view:
    /// the caller's roles and merged capabilities only, never other
    /// roles or rules. What `GET /permissions` serves.
    Effective {
        identity: transit_authz::Identity,
        #[ts(as = "transit_authz::Policy")]
        policy: cap_abi::Current<transit_authz::Policy>,
    },
    /// What a policy write must pass (the receive path of
    /// `POST /pointers/policy`): every issue that makes the policy
    /// wrong, or, when there is none, the canonical document and its
    /// hash. The consequences a person should see ride along in
    /// `warnings` and never refuse (a policy nobody can change again).
    Validate {
        #[ts(as = "transit_authz::Policy")]
        policy: cap_abi::Base<transit_authz::Policy>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_authz/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Authorize { decision: transit_authz::Decision },
    Effective { effective: transit_authz::EffectivePolicy },
    Validate {
        issues: Vec<transit_authz::PolicyIssue>,
        warnings: Vec<transit_authz::PolicyWarning>,
        document: Option<String>,
        document_hash: Option<String>,
        /// The fields the policy carries that this build does not know
        /// (D-167): kept in it, and flagged for a person, never refused.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, as = "Option<Vec<String>>")]
        unknown_fields: Vec<String>,
    },
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Authorize { identity, action, pointer, policy } => {
            let action = match action.as_str() {
                "read" => transit_authz::Action::Read,
                "write" => transit_authz::Action::Write,
                "audit" => transit_authz::Action::Audit,
                other => return Err(format!("unknown action {other:?}: want \"read\", \"write\" or \"audit\"")),
            };
            Ok(Response::Authorize {
                decision: transit_authz::authorize(&policy, &identity, action, &pointer),
            })
        }
        Request::Effective { identity, policy } => Ok(Response::Effective {
            effective: transit_authz::effective(&policy, &identity),
        }),
        Request::Validate { policy } => {
            let checked = transit_authz::validate(&policy.0);
            let (document, document_hash) = if checked.issues.is_empty() {
                let document = cap_abi::canonical_bytes(&policy.0)?;
                let hash = cap_abi::content_hash(document.as_bytes());
                (Some(document), Some(hash))
            } else {
                (None, None)
            };
            Ok(Response::Validate {
                issues: checked.issues,
                warnings: checked.warnings,
                document,
                document_hash,
                unknown_fields: cap_abi::request_unknown_fields(),
            })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// No bulk-bytes op exists — the policy is a JSON document.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: the policy document is JSON (authorize)") })
        .to_string()
}

/// The worked examples, which the generator runs (no module build turns
/// the feature on).
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "authz",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [],
    request: Request,
    speaks: &[("policy", &[1])],
}
