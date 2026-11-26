//! The work rules: what a labor agreement binds. The run rules and pay are
//! for the runs cut. The v1 to v2 step drops what moved to the scheduling
//! constraints.

use serde_json::json;
use transit_runs::version::{migrate_step, migrate_to};
use transit_runs::{MealBreak, Pay, RunRules, WorkRules, SCHEMA_VERSION};

#[test]
fn every_rule_is_optional() {
    let empty: WorkRules = serde_json::from_str(r#"{"kind":"work_rules","schema_version":2}"#).unwrap();
    assert_eq!(empty, WorkRules::default());
    assert_eq!(empty.clone().checked().unwrap(), empty);
    assert_eq!(serde_json::to_string(&empty).unwrap(), r#"{"kind":"work_rules","schema_version":2}"#, "nothing unset is written");
}

#[test]
fn the_whole_document_round_trips() {
    let rules = WorkRules {
        name: "Agency 2027".into(),
        runs: RunRules {
            max_platform_secs: Some(8 * 3600),
            max_spread_secs: Some(12 * 3600),
            report_secs: Some(900),
            clear_secs: Some(900),
            min_piece_secs: Some(2 * 3600),
            max_piece_secs: Some(5 * 3600),
            meal: Some(MealBreak { within_secs: 5 * 3600, min_secs: 1800, extra: Default::default() }),
            extra: Default::default(),
        },
        pay: Some(Pay {
            guarantee_secs: Some(8 * 3600),
            overtime_after_secs: Some(8 * 3600),
            overtime_pct: Some(150),
            spread_after_secs: Some(10 * 3600),
            spread_pct: Some(50),
            extra: Default::default(),
        }),
        ..Default::default()
    };
    let text = serde_json::to_string(&rules).unwrap();
    let back: WorkRules = serde_json::from_str(&text).unwrap();
    assert_eq!(back, rules);
    assert_eq!(back.checked().unwrap(), rules);
}

#[test]
fn a_document_that_cannot_be_read_one_way_is_refused() {
    let refused = |rules: WorkRules| rules.checked().unwrap_err();
    assert!(refused(WorkRules { schema_version: transit_version::Version(1), ..Default::default() }).contains("version"));
    assert!(refused(WorkRules {
        runs: RunRules { min_piece_secs: Some(10), max_piece_secs: Some(5), ..Default::default() },
        ..Default::default()
    })
    .contains("piece"));
}

#[test]
fn the_v1_step_drops_what_moved_to_the_scheduling_constraints() {
    let v1 = json!({"kind": "work_rules", "schema_version": 1, "name": "Agency",
        "recovery": [{"min_secs": 300}], "interline": [{"a": "5", "b": "6"}],
        "runs": {"report_secs": 900, "relief_points": ["A"]}, "pay": {"guarantee_secs": 28800}});
    let v2 = migrate_step(v1, 1).unwrap();
    assert_eq!(v2, json!({"kind": "work_rules", "schema_version": 2, "name": "Agency",
        "runs": {"report_secs": 900}, "pay": {"guarantee_secs": 28800}}));
    let read: WorkRules = serde_json::from_value(v2).unwrap();
    assert_eq!(read.checked().unwrap().runs.report_secs, Some(900));
}

#[test]
fn runs_left_empty_by_the_step_are_not_written() {
    let v2 = migrate_to(json!({"runs": {"relief_points": ["A"]}}), SCHEMA_VERSION).unwrap();
    assert_eq!(v2, json!({"schema_version": 2}));
}

#[test]
fn the_chain_refuses_what_it_cannot_step() {
    assert!(migrate_to(json!({"schema_version": 2}), 2).unwrap_err().contains("nothing to migrate"));
    assert!(migrate_to(json!({"schema_version": 1}), 3).unwrap_err().contains("newer than this build"));
    assert!(migrate_step(json!({"schema_version": 2}), 1).unwrap_err().contains("given a v2"));
    assert!(migrate_step(json!({"kind": "stops"}), 1).unwrap_err().contains("not a work rules document"));
}
