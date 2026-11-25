//! Authentication is the IdP's. The host verifies the JWT (signature, iss,
//! aud, exp; D-60). `authorize` fails closed: with no policy, no matching
//! rule, or a malformed request, the answer is a deny. The host owns
//! the token and the clock, and passes the policy document on each call.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const SCHEMA_VERSION: u32 = 1;

/// `"*"` in a rule matches all. `Audit` reads the pointer log with writer
/// identities, the only place a user identity leaves the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Read,
    Write,
    Audit,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Read => "read",
            Action::Write => "write",
            Action::Audit => "audit",
        }
    }
}

/// The subject and the roles from the configured claim path (D-59). Nothing
/// else is trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Identity {
    pub sub: String,
    pub roles: Vec<String>,
}

/// `"*"` matches all actions or pointers. A rule with empty `roles` never
/// matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Rule {
    pub name: String,
    pub roles: Vec<String>,
    #[serde(default)]
    pub pointers: Vec<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl Rule {
    fn matches(&self, action: Action, pointer: &str) -> bool {
        let action_ok = self.actions.is_empty()
            || self.actions.iter().any(|a| a == "*" || a == action.as_str());
        let pointer_ok = self.pointers.is_empty()
            || self.pointers.iter().any(|p| p == "*" || p == pointer);
        action_ok && pointer_ok
    }
}

transit_version::kind_tag!(
    PolicyKind = "policy"
);

impl transit_version::Versioned for Policy {
    const KIND: &'static str = "policy";
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Policy {
    /// A copy without a kind tag reads as a policy (D-115).
    #[serde(default)]
    #[ts(type = "\"policy\"")]
    pub kind: PolicyKind,
    pub schema_version: u32,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl Policy {
    pub fn current(rules: Vec<Rule>) -> Self {
        Policy { kind: PolicyKind, schema_version: SCHEMA_VERSION, rules, extra: Default::default() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Decision {
    pub allow: bool,
    pub matched_rule: Option<String>,
    /// Safe to log. It never echoes the token.
    pub reason: String,
}

/// A rule matches only when the identity holds one of its roles and its
/// actions and pointers match. Empty actions or pointers mean all, and empty
/// roles mean never. The verdict does not depend on rule order, though the
/// reported rule name may.
pub fn authorize(policy: &Policy, identity: &Identity, action: Action, pointer: &str) -> Decision {
    if identity.sub.is_empty() {
        return Decision { allow: false, matched_rule: None, reason: "no subject".into() };
    }
    for rule in &policy.rules {
        let role_ok = !rule.roles.is_empty()
            && rule.roles.iter().any(|r| identity.roles.contains(r));
        if role_ok && rule.matches(action, pointer) {
            return Decision {
                allow: true,
                matched_rule: Some(rule.name.clone()),
                reason: format!("rule {:?} grants {} on {} to a held role", rule.name, action.as_str(), pointer),
            };
        }
    }
    Decision {
        allow: false,
        matched_rule: None,
        reason: format!("no rule grants {} on {} to the identity's roles", action.as_str(), pointer),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Capability {
    pub pointer: String,
    pub actions: Vec<String>,
}

/// A client drives its UI from it without learning anything about other
/// roles, other rules, or the rest of the policy's structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct EffectivePolicy {
    pub roles: Vec<String>,
    pub capabilities: Vec<Capability>,
}

/// Capabilities are sorted by pointer, and actions sorted and deduped.
pub fn effective(policy: &Policy, identity: &Identity) -> EffectivePolicy {
    let mut caps: Vec<Capability> = Vec::new();
    for rule in &policy.rules {
        if rule.roles.is_empty() || !rule.roles.iter().any(|r| identity.roles.contains(r)) {
            continue;
        }
        let actions = if rule.actions.is_empty() { vec!["*".to_string()] } else { rule.actions.clone() };
        let pointers = if rule.pointers.is_empty() { vec!["*".to_string()] } else { rule.pointers.clone() };
        for p in pointers {
            match caps.iter_mut().find(|c| c.pointer == p) {
                Some(c) => {
                    for a in &actions {
                        if !c.actions.contains(a) {
                            c.actions.push(a.clone());
                        }
                    }
                }
                None => caps.push(Capability { pointer: p, actions: actions.clone() }),
            }
        }
    }
    for c in &mut caps {
        c.actions.sort();
        c.actions.dedup();
    }
    caps.sort_by(|a, b| a.pointer.cmp(&b.pointer));
    EffectivePolicy { roles: identity.roles.clone(), capabilities: caps }
}

#[cfg(test)]
mod laws {
    use super::*;

    fn id(roles: &[&str]) -> Identity {
        Identity { sub: "user-1".into(), roles: roles.iter().map(|r| r.to_string()).collect() }
    }

    fn rule(name: &str, roles: &[&str], pointers: &[&str], actions: &[&str]) -> Rule {
        Rule {
            name: name.into(),
            roles: roles.iter().map(|r| r.to_string()).collect(),
            pointers: pointers.iter().map(|p| p.to_string()).collect(),
            actions: actions.iter().map(|a| a.to_string()).collect(),
            extra: Default::default(),
        }
    }

    #[test]
    fn empty_policy_denies_everything() {
        let policy = Policy::current(vec![]);
        for action in [Action::Read, Action::Write] {
            let d = authorize(&policy, &id(&["admin"]), action, "edits");
            assert!(!d.allow, "fail-closed: {d:?}");
        }
    }

    #[test]
    fn a_rule_matches_only_on_role_action_and_pointer() {
        let policy = Policy::current(vec![rule("ops", &["dispatcher"], &["edits"], &["write"])]);
        assert!(authorize(&policy, &id(&["dispatcher"]), Action::Write, "edits").allow);
        assert!(!authorize(&policy, &id(&["dispatcher"]), Action::Read, "edits").allow, "write does not imply read");
        assert!(!authorize(&policy, &id(&["dispatcher"]), Action::Write, "policy").allow, "other pointer");
        assert!(!authorize(&policy, &id(&["rider"]), Action::Write, "edits").allow, "role not held");
    }

    #[test]
    fn empty_roles_in_a_rule_never_match_and_wildcards_do() {
        let policy = Policy::current(vec![
            rule("nobody", &[], &["*"], &["*"]),
            rule("all-read", &["rider"], &["*"], &["read"]),
            rule("admin-all", &["admin"], &["*"], &["*"]),
        ]);
        assert!(!authorize(&policy, &id(&["rider"]), Action::Write, "edits").allow);
        assert!(authorize(&policy, &id(&["rider"]), Action::Read, "anything").allow, "* pointer, read action");
        assert!(authorize(&policy, &id(&["admin"]), Action::Write, "policy").allow, "*/* for admin");
        assert!(!authorize(&policy, &id(&[]), Action::Read, "x").allow, "identity with no roles");
    }

    #[test]
    fn no_subject_is_a_deny() {
        let policy = Policy::current(vec![rule("all", &["admin"], &["*"], &["*"])]);
        let anon = Identity { sub: "".into(), roles: vec!["admin".into()] };
        assert!(!authorize(&policy, &anon, Action::Write, "edits").allow);
    }

    #[test]
    fn the_verdict_is_rule_order_insensitive() {
        let rules = vec![
            rule("a", &["ops"], &["edits"], &["write"]),
            rule("b", &["ops", "admin"], &["*"], &["*"]),
        ];
        let p1 = Policy::current(rules.clone());
        let mut p2 = Policy::current(rules.clone());
        p2.rules.reverse();
        for action in [Action::Read, Action::Write] {
            for pointer in ["edits", "policy", "manifest"] {
                for roles in [vec!["ops"], vec!["admin"], vec!["rider"]] {
                    let i = id(&roles.iter().map(|s| *s).collect::<Vec<_>>());
                    let d1 = authorize(&p1, &i, action, pointer);
                    let d2 = authorize(&p2, &i, action, pointer);
                    assert_eq!(d1.allow, d2.allow, "order changed the verdict for {roles:?} {action:?} {pointer}");
                }
            }
        }
    }

    #[test]
    fn policy_round_trips_through_json() {
        let policy = Policy::current(vec![rule("ops", &["dispatcher"], &["edits"], &["write", "read"])]);
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("\"schema_version\":1"), "{json}");
        assert_eq!(serde_json::from_str::<Policy>(&json).unwrap(), policy);
    }

    #[test]
    fn effective_projects_only_the_identitys_own_rules() {
        let policy = Policy::current(vec![
            rule("ops", &["dispatcher"], &["edits"], &["write", "read"]),
            rule("auditors", &["auditor"], &["*"], &["audit"]),
            rule("admin-all", &["admin"], &["*"], &["*"]),
        ]);
        let eff = effective(&policy, &id(&["dispatcher"]));
        assert_eq!(eff.roles, vec!["dispatcher".to_string()]);
        assert_eq!(eff.capabilities, vec![Capability {
            pointer: "edits".into(),
            actions: vec!["read".into(), "write".into()],
        }]);
        let eff = effective(&policy, &id(&["auditor"]));
        assert_eq!(eff.capabilities, vec![Capability { pointer: "*".into(), actions: vec!["audit".into()] }]);
        assert!(effective(&Policy::current(vec![]), &id(&["admin"])).capabilities.is_empty());
        assert!(effective(&policy, &id(&[])).capabilities.is_empty());
    }

    #[test]
    fn effective_merges_wildcards_and_is_deterministic() {
        let policy = Policy::current(vec![
            rule("a", &["ops"], &["edits", "*"], &["write"]),
            rule("b", &["ops"], &["edits"], &["read", "write"]),
        ]);
        let eff = effective(&policy, &id(&["ops"]));
        assert_eq!(eff.capabilities.len(), 2, "edits merged, * separate");
        let edits = eff.capabilities.iter().find(|c| c.pointer == "edits").unwrap();
        assert_eq!(edits.actions, vec!["read".to_string(), "write".to_string()], "sorted, deduped");
        let star = eff.capabilities.iter().find(|c| c.pointer == "*").unwrap();
        assert_eq!(star.actions, vec!["write".to_string()]);
    }
}

#[cfg(test)]
mod prop {
    use super::*;
    use proptest::prelude::*;
    use proptest::collection::vec;

    fn role_name() -> impl Strategy<Value = String> {
        proptest::sample::select(vec!["admin".to_string(), "dispatcher".to_string(), "rider".to_string()])
    }

    fn rule_strat() -> impl Strategy<Value = Rule> {
        (
            "[a-z]{1,6}",
            vec(role_name(), 0..3),
            vec(proptest::sample::select(vec!["*".to_string(), "edits".to_string(), "policy".to_string(), "manifest".to_string()]), 0..3),
            vec(proptest::sample::select(vec!["*".to_string(), "read".to_string(), "write".to_string()]), 0..3),
        )
            .prop_map(|(name, roles, pointers, actions)| Rule { name, roles, pointers, actions, extra: Default::default() })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// A deny is the only answer when no rule's roles intersect the
        /// identity's.
        #[test]
        fn deny_when_no_role_intersects(
            rules in vec(rule_strat(), 0..5),
            identity_roles in vec(role_name(), 0..3),
            action in proptest::sample::select(vec![Action::Read, Action::Write, Action::Audit]),
            pointer in proptest::sample::select(vec!["edits", "policy", "manifest", "plans"]),
        ) {
            let policy = Policy::current(rules);
            let identity = Identity { sub: "u".into(), roles: identity_roles };
            let d = authorize(&policy, &identity, action, pointer);
            let any_role_overlap = policy.rules.iter().any(|r| {
                !r.roles.is_empty() && r.roles.iter().any(|x| identity.roles.contains(x))
            });
            if !any_role_overlap {
                prop_assert!(!d.allow, "allowed with no role overlap: {d:?}");
            }
        }

        /// The verdict does not depend on rule order.
        #[test]
        fn verdict_is_order_insensitive(
            rules in vec(rule_strat(), 0..5),
            identity_roles in vec(role_name(), 0..3),
            action in proptest::sample::select(vec![Action::Read, Action::Write, Action::Audit]),
            pointer in "[a-z]+",
        ) {
            let mut reversed = rules.clone();
            reversed.reverse();
            let p1 = Policy::current(rules);
            let p2 = Policy::current(reversed);
            let identity = Identity { sub: "u".into(), roles: identity_roles };
            prop_assert_eq!(
                authorize(&p1, &identity, action, &pointer).allow,
                authorize(&p2, &identity, action, &pointer).allow,
            );
        }

        /// The projection is sorted, so it is stable under rule permutation.
        #[test]
        fn effective_is_order_insensitive(
            rules in vec(rule_strat(), 0..5),
            identity_roles in vec(role_name(), 0..3),
        ) {
            let mut reversed = rules.clone();
            reversed.reverse();
            let identity = Identity { sub: "u".into(), roles: identity_roles };
            prop_assert_eq!(
                effective(&Policy::current(rules), &identity),
                effective(&Policy::current(reversed), &identity),
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PolicyIssue {
    UnknownAction { rule: String, action: String },
    UnnamedRule { index: usize },
    DuplicateRuleName { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PolicyWarning {
    NoPolicyWriter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct PolicyCheck {
    pub issues: Vec<PolicyIssue>,
    pub warnings: Vec<PolicyWarning>,
}

pub fn validate(policy: &Policy) -> PolicyCheck {
    let mut issues = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut duplicated = std::collections::BTreeSet::new();
    for (index, rule) in policy.rules.iter().enumerate() {
        if rule.name.trim().is_empty() {
            issues.push(PolicyIssue::UnnamedRule { index });
        } else if !seen.insert(rule.name.as_str()) && duplicated.insert(rule.name.as_str()) {
            issues.push(PolicyIssue::DuplicateRuleName { name: rule.name.clone() });
        }
        for action in &rule.actions {
            if !matches!(action.as_str(), "read" | "write" | "audit" | "*") {
                issues.push(PolicyIssue::UnknownAction { rule: rule.name.clone(), action: action.clone() });
            }
        }
    }
    let writer = policy.rules.iter().any(|r| !r.roles.is_empty() && r.matches(Action::Write, "policy"));
    let warnings = if writer { vec![] } else { vec![PolicyWarning::NoPolicyWriter] };
    PolicyCheck { issues, warnings }
}
