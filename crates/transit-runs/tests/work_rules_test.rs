//! The work rules: what a labor agreement binds. The model over generated
//! documents is `labor_rules_prop_test`. Here: an empty document, and the
//! migration chain.

use serde_json::json;
use transit_runs::version::{migrate_step, migrate_to};
use transit_runs::{WorkRules, SCHEMA_VERSION};

#[test]
fn every_rule_is_optional() {
    let empty: WorkRules =
        serde_json::from_str(r#"{"kind":"work_rules","schema_version":3}"#).unwrap();
    assert_eq!(empty, WorkRules::default());
    assert_eq!(empty.clone().checked().unwrap(), empty);
    assert_eq!(
        serde_json::to_string(&empty).unwrap(),
        r#"{"kind":"work_rules","schema_version":3}"#,
        "nothing unset is written"
    );
}

#[test]
fn another_version_is_refused() {
    for v in [1, 2, 4] {
        let rules = WorkRules {
            schema_version: transit_version::Version(v),
            ..Default::default()
        };
        assert!(rules.checked().unwrap_err().contains("version"));
    }
}

#[test]
fn the_v1_step_drops_what_moved_to_the_scheduling_constraints() {
    let v1 = json!({"kind": "work_rules", "schema_version": 1, "name": "Agency",
        "recovery": [{"min_secs": 300}], "interline": [{"a": "5", "b": "6"}],
        "runs": {"report_secs": 900, "relief_points": ["A"]}, "pay": {"guarantee_secs": 28800}});
    let v2 = migrate_step(v1, 1).unwrap();
    assert_eq!(
        v2,
        json!({"kind": "work_rules", "schema_version": 2, "name": "Agency",
        "runs": {"report_secs": 900}, "pay": {"guarantee_secs": 28800}})
    );
    assert_eq!(
        migrate_step(v2, 2).unwrap(),
        json!({"kind": "work_rules", "schema_version": 3, "name": "Agency"})
    );
}

#[test]
fn the_v2_step_drops_the_run_rules_and_pay() {
    let v2 = json!({"kind": "work_rules", "schema_version": 2, "name": "Agency",
        "runs": {"max_platform_secs": 28800}, "pay": {"guarantee_secs": 28800}, "unknown": 1});
    let v3 = migrate_step(v2, 2).unwrap();
    assert_eq!(
        v3,
        json!({"kind": "work_rules", "schema_version": 3, "name": "Agency", "unknown": 1})
    );
    let read: WorkRules = serde_json::from_value(v3).unwrap();
    assert_eq!(read.checked().unwrap().name, "Agency");
}

#[test]
fn runs_left_empty_by_the_step_are_not_written() {
    let v2 = migrate_step(json!({"runs": {"relief_points": ["A"]}}), 1).unwrap();
    assert_eq!(v2, json!({"schema_version": 2}));
    assert_eq!(
        migrate_to(json!({}), SCHEMA_VERSION).unwrap(),
        json!({"schema_version": 3})
    );
}

#[test]
fn the_chain_refuses_what_it_cannot_step() {
    assert!(migrate_to(json!({"schema_version": 3}), 3)
        .unwrap_err()
        .contains("nothing to migrate"));
    assert!(migrate_to(json!({"schema_version": 1}), 4)
        .unwrap_err()
        .contains("newer than this build"));
    assert!(migrate_step(json!({"schema_version": 2}), 1)
        .unwrap_err()
        .contains("given a v2"));
    assert!(migrate_step(json!({"kind": "stops"}), 1)
        .unwrap_err()
        .contains("not a work rules document"));
}
