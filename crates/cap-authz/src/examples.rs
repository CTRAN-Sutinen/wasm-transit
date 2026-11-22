//! The authorization capability's worked examples (its `examples`
//! feature): each request. The generator runs them through this crate's
//! `invoke` and records the answers.

use cap_abi::manifest::Example;
use serde_json::json;
use transit_authz::{Identity, Policy, Rule};

fn rule(name: &str, role: &str, pointer: &str, actions: &[&str]) -> Rule {
    Rule { name: name.into(), roles: vec![role.into()], pointers: vec![pointer.into()], actions: actions.iter().map(|a| a.to_string()).collect(), extra: Default::default() }
}

pub fn examples() -> Vec<Example> {
    let dispatcher = Identity { sub: "dispatcher-7".into(), roles: vec!["dispatcher".into()] };
    let two = Policy::current(vec![rule("ops-write-edits", "dispatcher", "edits", &["read", "write"]), rule("admin-all", "admin", "*", &["*"])]);
    vec![
        // The fail-closed decision.
        Example {
            bytes: None, kinds: vec![],
            op: "authorize",
            loads: vec![],
            request: json!({ "op": "authorize", "identity": dispatcher, "action": "write", "pointer": "edits",
                             "policy": Policy::current(vec![rule("ops-write-edits", "dispatcher", "edits", &["write"])]) }),
        },
        Example { bytes: None, kinds: vec![], op: "effective", loads: vec![], request: json!({ "op": "effective", "identity": dispatcher, "policy": two }) },
        Example { bytes: None, kinds: vec![], op: "validate", loads: vec![], request: json!({ "op": "validate", "policy": two }) },
    ]
}
