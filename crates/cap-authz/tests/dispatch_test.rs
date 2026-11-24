//! Native dispatch tests for the authorization capability. These test the
//! seam without wasm. The laws (fail-closed, order-insensitive verdict,
//! role/action/pointer matching) are property-tested in transit-authz.
//! Here the seam pins them through the module:
//!
//! 1. a matching rule allows. Anything else denies: an empty policy, an
//!    unknown action, an unknown pointer, or a missing role all deny;
//! 2. an unknown action string is an error, never a silent allow;
//! 3. garbage and bulk bytes are honest errors, never panics;
//! 4. a policy in another version is refused. The error names both.

use cap_authz::{dispatch, dispatch_bytes, dispatch_json, Request, Response};
use transit_authz::{Identity, Policy, Rule};

fn policy() -> Policy {
    Policy::current(vec![
        Rule {
            name: "ops-write".into(),
            roles: vec!["dispatcher".into()],
            pointers: vec!["edits".into()],
            actions: vec!["write".into()],
            extra: Default::default(),
        },
        Rule {
            name: "admin-all".into(),
            roles: vec!["admin".into()],
            pointers: vec!["*".into()],
            actions: vec!["*".into()],
            extra: Default::default(),
        },
    ])
}

fn identity(roles: &[&str]) -> Identity {
    Identity { sub: "user-1".into(), roles: roles.iter().map(|r| r.to_string()).collect() }
}

fn ask(id: &Identity, action: &str, pointer: &str) -> bool {
    match dispatch(Request::Authorize {
        identity: id.clone(),
        action: action.into(),
        pointer: pointer.into(),
        policy: cap_abi::Current(policy()),
    })
    .unwrap()
    {
        Response::Authorize { decision } => decision.allow,
        _ => panic!("authorize"),
    }
}

#[test]
fn matching_rule_allows_and_everything_else_denies() {
    assert!(ask(&identity(&["dispatcher"]), "write", "edits"), "rule ops-write");
    assert!(!ask(&identity(&["dispatcher"]), "read", "edits"), "write does not imply read");
    assert!(!ask(&identity(&["rider"]), "write", "edits"), "role not held");
    assert!(!ask(&identity(&["dispatcher"]), "write", "policy"), "pointer not in the rule");
    assert!(ask(&identity(&["admin"]), "write", "policy"), "admin wildcard");
    assert!(ask(&identity(&["admin"]), "read", "manifest"), "admin read anywhere");
}

#[test]
fn empty_policy_is_fail_closed() {
    match dispatch(Request::Authorize {
        identity: identity(&["admin"]),
        action: "write".into(),
        pointer: "edits".into(),
        policy: cap_abi::Current(Policy::current(vec![])),
    })
    .unwrap()
    {
        Response::Authorize { decision } => {
            assert!(!decision.allow);
            assert_eq!(decision.matched_rule, None);
            assert!(!decision.reason.is_empty());
        }
        _ => panic!("authorize"),
    }
}

#[test]
fn effective_projects_only_the_callers_own_capabilities() {
    match dispatch(Request::Effective { identity: identity(&["dispatcher"]), policy: cap_abi::Current(policy()) }).unwrap() {
        Response::Effective { effective } => {
            assert_eq!(effective.roles, vec!["dispatcher".to_string()]);
            // Its own merged capabilities. Nothing about admin.
            assert_eq!(effective.capabilities.len(), 1);
            assert_eq!(effective.capabilities[0].pointer, "edits");
            assert_eq!(effective.capabilities[0].actions, vec!["write".to_string()]);
            let json = serde_json::to_string(&effective).unwrap();
            assert!(!json.contains("admin"), "no other roles leak: {json}");
        }
        _ => panic!("effective"),
    }
    // An empty policy gives an empty projection (fail-closed).
    match dispatch(Request::Effective { identity: identity(&["admin"]), policy: cap_abi::Current(Policy::current(vec![])) }).unwrap() {
        Response::Effective { effective } => assert!(effective.capabilities.is_empty()),
        _ => panic!("effective"),
    }
}

#[test]
fn unknown_action_is_an_error_not_an_allow() {
    let err = dispatch(Request::Authorize {
        identity: identity(&["admin"]),
        action: "delete-everything".into(),
        pointer: "edits".into(),
        policy: cap_abi::Current(policy()),
    })
    .unwrap_err();
    assert!(err.contains("unknown action"), "{err}");

    let bad = dispatch_json(r#"{"op":"authorize","identity":{"sub":"u","roles":["admin"]},"action":"write","pointer":"edits","policy":{"schema_version":1,"rules":[]}}"#);
    assert!(bad.contains("\"allow\":false"), "empty rules deny through the wire: {bad}");
    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_bytes(0, b"x");
    assert!(bad.contains("unknown bulk op"), "{bad}");
}

/// A policy in another version is refused. The error names both versions.
/// It is never read as this build's (fail-closed; docs/FORMATS.md, "Versions").
#[test]
fn a_policy_in_another_version_is_refused_naming_both() {
    let request = serde_json::json!({
        "op": "authorize",
        "identity": {"sub": "u1", "roles": ["dispatcher"]},
        "action": "write",
        "pointer": "edits",
        "policy": {"kind": "policy", "schema_version": 2, "rules": []},
    });
    let answer = cap_authz::dispatch_json(&request.to_string());
    assert!(answer.contains("policy v2") && answer.contains("reads v1"), "{answer}");
}

#[test]
fn a_policy_write_is_checked_through_the_seam() {
    let answer = |doc: &str| -> serde_json::Value {
        let out: serde_json::Value =
            serde_json::from_str(&cap_authz::dispatch_json(&format!(r#"{{"op":"validate","policy":{doc}}}"#))).unwrap();
        out["ok"].clone()
    };
    let sound = answer(r#"{"schema_version":1,"rules":[{"name":"admin","roles":["admin"],"pointers":["*"],"actions":["*"]}],"note":"kept"}"#);
    assert!(sound["issues"].as_array().unwrap().is_empty(), "{sound}");
    assert!(sound["document"].as_str().unwrap().contains(r#""note":"kept""#), "a field this build does not know is kept: {sound}");
    assert_eq!(sound["unknown_fields"], serde_json::json!(["note"]));
    let wrong = answer(r#"{"schema_version":1,"rules":[{"name":"ops","roles":["d"],"pointers":["edits"],"actions":["wirte"]}]}"#);
    assert_eq!(wrong["issues"][0]["kind"], "unknown_action");
    assert!(wrong["document"].is_null());
    assert_eq!(wrong["warnings"][0]["kind"], "no_policy_writer");
    let newer: serde_json::Value =
        serde_json::from_str(&cap_authz::dispatch_json(r#"{"op":"validate","policy":{"schema_version":9,"rules":[]}}"#)).unwrap();
    assert!(newer.get("error").is_some(), "a newer policy is refused, naming both versions: {newer}");
}
