//! The IdP authenticates the identity. The host verifies the JWT and
//! reads the roles claim. This module owns only the decision, over
//! the policy document (kind `policy`) that the host pushes on each
//! call, and the module never caches across calls. `validate` is the
//! policy's check in the kind table (D-202). transit-authz holds the
//! property tests.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_authz/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Authorize {
        identity: transit_authz::Identity,
        action: String,
        pointer: String,
        #[cfg_attr(feature = "bindings", ts(as = "transit_authz::Policy"))]
        policy: cap_abi::Current<transit_authz::Policy>,
    },
    Effective {
        identity: transit_authz::Identity,
        #[cfg_attr(feature = "bindings", ts(as = "transit_authz::Policy"))]
        policy: cap_abi::Current<transit_authz::Policy>,
    },
    Validate {
        #[cfg_attr(feature = "bindings", ts(as = "transit_authz::Policy"))]
        policy: cap_abi::Base<transit_authz::Policy>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_authz/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Authorize {
        decision: transit_authz::Decision,
    },
    Effective {
        effective: transit_authz::EffectivePolicy,
    },
    Validate {
        issues: Vec<transit_authz::PolicyIssue>,
        warnings: Vec<transit_authz::PolicyWarning>,
        document: Option<String>,
        document_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<String>>"))]
        unknown_fields: Vec<String>,
    },
}

pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Authorize {
            identity,
            action,
            pointer,
            policy,
        } => {
            let action = match action.as_str() {
                "read" => transit_authz::Action::Read,
                "write" => transit_authz::Action::Write,
                "audit" => transit_authz::Action::Audit,
                other => {
                    return Err(format!(
                        "unknown action {other:?}: want \"read\", \"write\" or \"audit\""
                    ))
                }
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

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": { "message": format!("unknown bulk op {op}: the policy document is JSON (authorize)") } })
        .to_string()
}

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
