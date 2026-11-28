//! A policy write is refused only for what makes the policy wrong, and a
//! consequence is accepted and named for a person (docs/architecture/
//! agency-data.md, "Writes are governed"). The checks do not depend on rule
//! order.

use transit_authz::{validate, Policy, PolicyIssue, PolicyWarning, Rule};

fn rule(name: &str, roles: &[&str], pointers: &[&str], actions: &[&str]) -> Rule {
    Rule {
        name: name.into(),
        roles: roles.iter().map(|s| s.to_string()).collect(),
        pointers: pointers.iter().map(|s| s.to_string()).collect(),
        actions: actions.iter().map(|s| s.to_string()).collect(),
        extra: Default::default(),
    }
}

fn admin() -> Rule {
    rule("admin-all", &["admin"], &["*"], &["*"])
}

#[test]
fn a_sound_policy_passes() {
    let p = Policy::current(vec![
        admin(),
        rule(
            "ops",
            &["dispatcher"],
            &["edits"],
            &["read", "write", "audit"],
        ),
    ]);
    let v = validate(&p);
    assert!(v.issues.is_empty(), "{:?}", v.issues);
    assert!(v.warnings.is_empty(), "{:?}", v.warnings);
}

#[test]
fn an_unknown_action_is_refused() {
    let p = Policy::current(vec![
        admin(),
        rule("ops", &["dispatcher"], &["edits"], &["wirte"]),
    ]);
    assert_eq!(
        validate(&p).issues,
        vec![PolicyIssue::UnknownAction {
            rule: "ops".into(),
            action: "wirte".into()
        }]
    );
}

#[test]
fn a_rule_is_named_once() {
    let p = Policy::current(vec![
        admin(),
        rule("", &["a"], &[], &[]),
        rule("admin-all", &["b"], &[], &["read"]),
    ]);
    let issues = validate(&p).issues;
    assert!(
        issues.contains(&PolicyIssue::UnnamedRule { index: 1 }),
        "{issues:?}"
    );
    assert!(
        issues.contains(&PolicyIssue::DuplicateRuleName {
            name: "admin-all".into()
        }),
        "{issues:?}"
    );
}

#[test]
fn a_policy_nobody_can_change_is_accepted_and_named() {
    let p = Policy::current(vec![rule("ops", &["dispatcher"], &["edits"], &["write"])]);
    let v = validate(&p);
    assert!(v.issues.is_empty());
    assert_eq!(v.warnings, vec![PolicyWarning::NoPolicyWriter]);
    let p = Policy::current(vec![rule("nobody", &[], &["policy"], &["write"])]);
    assert_eq!(validate(&p).warnings, vec![PolicyWarning::NoPolicyWriter]);
    let p = Policy::current(vec![rule("root", &["admin"], &[], &[])]);
    assert!(validate(&p).warnings.is_empty());
}

#[test]
fn the_checks_never_depend_on_rule_order() {
    let rules = vec![
        admin(),
        rule("x", &["a"], &[], &["bogus"]),
        rule("x", &["b"], &[], &["read"]),
        rule("", &[], &[], &[]),
    ];
    let forward = validate(&Policy::current(rules.clone()));
    let mut reversed = rules;
    reversed.reverse();
    let backward = validate(&Policy::current(reversed));
    let sorted = |mut v: Vec<PolicyIssue>| {
        v.sort_by_key(|i| format!("{i:?}"));
        v
    };
    // An unnamed rule is named by its place, which moves with the order.
    let unplaced = |v: Vec<PolicyIssue>| {
        v.into_iter()
            .filter(|i| !matches!(i, PolicyIssue::UnnamedRule { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sorted(unplaced(forward.issues)),
        sorted(unplaced(backward.issues))
    );
    assert_eq!(forward.warnings, backward.warnings);
}
