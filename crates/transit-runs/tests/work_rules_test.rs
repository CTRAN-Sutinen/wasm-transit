//! The work-rule document (D-50): generic, every rule optional, an
//! agency's values as data. Its recovery requirement (D-9) and its
//! interlining (D-46, per route pair) are what blocking reads today;
//! the run rules (platform, spread, report and clear, pieces, meals,
//! relief points, pay) are what the runs cut will read.
//!
//! API notes: `WorkRules::checked()` validates and normalizes (rules
//! and pairs sorted, each pair's routes in order) — the kernel's load
//! runs it, so two orderings of one document are one identity;
//! `recovery_secs(&RecoveryAt)` answers the most specific rule's
//! requirement; `interlines(a, b)`; `required(Option<&WorkRules>)`
//! is the closure `validation_issues_under` takes.

use proptest::prelude::*;
use transit_network::RecoveryAt;
use transit_runs::{MealBreak, Pay, RecoveryRule, RoutePair, RunRules, WorkRules, SCHEMA_VERSION};

fn rule(route: Option<&str>, service_type: Option<&str>, min_secs: Option<u32>, pct_of_run: Option<u32>) -> RecoveryRule {
    RecoveryRule { route: route.map(String::from), service_type: service_type.map(String::from), min_secs, pct_of_run }
}

fn pair(a: &str, b: &str) -> RoutePair {
    RoutePair { a: a.into(), b: b.into() }
}

fn at<'a>(route: &'a str, service_type: &'a str, run_secs: u32) -> RecoveryAt<'a> {
    RecoveryAt { route, service_type, run_secs, terminal: "C" }
}

#[test]
fn every_rule_is_optional() {
    let empty: WorkRules = serde_json::from_str(r#"{"kind":"work_rules","version":1}"#).unwrap();
    assert_eq!(empty.clone().checked().unwrap(), empty);
    assert_eq!(empty.recovery_secs(&at("5", "wk", 1200)), None);
    assert!(empty.interlines("5", "5"), "a route always follows itself");
    assert!(!empty.interlines("5", "6"), "no pair listed, no interlining");
    assert_eq!(serde_json::to_string(&empty).unwrap(), r#"{"kind":"work_rules","version":1}"#, "nothing unset is written");
}

#[test]
fn the_whole_document_round_trips() {
    let rules = WorkRules { kind: Default::default(),
        version: SCHEMA_VERSION,
        name: "Agency 2027".into(),
        recovery: vec![rule(None, None, Some(300), Some(10))],
        interline: vec![pair("5", "6")],
        runs: RunRules {
            max_platform_secs: Some(8 * 3600),
            max_spread_secs: Some(12 * 3600),
            report_secs: Some(900),
            clear_secs: Some(900),
            min_piece_secs: Some(2 * 3600),
            max_piece_secs: Some(5 * 3600),
            meal: Some(MealBreak { within_secs: 5 * 3600, min_secs: 1800 }),
            relief_points: vec!["A".into()],
        },
        pay: Some(Pay {
            guarantee_secs: Some(8 * 3600),
            overtime_after_secs: Some(8 * 3600),
            overtime_pct: Some(150),
            spread_after_secs: Some(10 * 3600),
            spread_pct: Some(50),
        }),
    };
    let text = serde_json::to_string(&rules).unwrap();
    let back: WorkRules = serde_json::from_str(&text).unwrap();
    assert_eq!(back, rules);
    assert_eq!(back.checked().unwrap(), rules);
}

#[test]
fn the_most_specific_rule_answers() {
    let rules = WorkRules {
        version: 1,
        recovery: vec![
            rule(None, None, Some(300), None),
            rule(None, Some("sat"), Some(400), None),
            rule(Some("5"), None, Some(500), None),
            rule(Some("5"), Some("sat"), Some(600), None),
        ],
        ..Default::default()
    };
    assert_eq!(rules.recovery_secs(&at("9", "wk", 1200)), Some(300));
    assert_eq!(rules.recovery_secs(&at("9", "sat", 1200)), Some(400));
    assert_eq!(rules.recovery_secs(&at("5", "wk", 1200)), Some(500));
    assert_eq!(rules.recovery_secs(&at("5", "sat", 1200)), Some(600));
}

proptest! {
    /// Minutes and a percentage of the run: the larger, the percentage
    /// rounded up to the second.
    #[test]
    fn a_rule_requires_the_larger_of_its_minimum_and_its_share(min in proptest::option::of(0u32..2000), pct in proptest::option::of(0u32..100), run in 0u32..10_000) {
        prop_assume!(min.is_some() || pct.is_some());
        let rules = WorkRules { version: 1, recovery: vec![rule(None, None, min, pct)], ..Default::default() };
        let share = pct.map(|p| (run * p).div_ceil(100));
        let want = min.into_iter().chain(share).max();
        prop_assert_eq!(rules.recovery_secs(&at("5", "wk", run)), want);
    }

    /// One document, however its rules and pairs are ordered.
    #[test]
    fn order_is_not_identity(swap in any::<bool>(), flip in any::<bool>()) {
        let mut recovery = vec![rule(Some("5"), None, Some(500), None), rule(None, None, Some(300), None)];
        let mut interline = vec![pair("6", "7"), if flip { pair("6", "5") } else { pair("5", "6") }];
        if swap {
            recovery.reverse();
            interline.reverse();
        }
        let a = WorkRules { version: 1, recovery, interline, ..Default::default() }.checked().unwrap();
        let b = WorkRules {
            version: 1,
            recovery: vec![rule(None, None, Some(300), None), rule(Some("5"), None, Some(500), None)],
            interline: vec![pair("5", "6"), pair("6", "7")],
            ..Default::default()
        };
        prop_assert_eq!(a, b);
    }
}

#[test]
fn interlining_is_per_route_pair_either_way() {
    let rules = WorkRules { version: 1, interline: vec![pair("5", "6")], ..Default::default() };
    assert!(rules.interlines("5", "6"));
    assert!(rules.interlines("6", "5"));
    assert!(!rules.interlines("5", "7"));
    assert!(rules.interlines("7", "7"));
}

#[test]
fn the_requirement_closure_falls_back_with_no_document() {
    let rules = WorkRules { version: 1, recovery: vec![rule(None, None, Some(300), None)], ..Default::default() };
    assert_eq!(transit_runs::required(Some(&rules))(&at("5", "wk", 1)), Some(300));
    assert_eq!(transit_runs::required(None)(&at("5", "wk", 1)), None);
}

#[test]
fn a_document_that_cannot_be_read_one_way_is_refused() {
    let refused = |rules: WorkRules| rules.checked().unwrap_err();
    let base = || WorkRules { version: 1, ..Default::default() };
    assert!(refused(WorkRules { version: 2, ..base() }).contains("version"));
    assert!(refused(WorkRules { recovery: vec![rule(None, None, None, None)], ..base() }).contains("requires nothing"));
    assert!(refused(WorkRules {
        recovery: vec![rule(Some("5"), None, Some(1), None), rule(Some("5"), None, Some(2), None)],
        ..base()
    })
    .contains("twice"));
    assert!(refused(WorkRules { recovery: vec![rule(None, None, None, Some(1001))], ..base() }).contains("percent"));
    assert!(refused(WorkRules { interline: vec![pair("5", "5")], ..base() }).contains("two routes"));
    assert!(refused(WorkRules { interline: vec![pair("5", "6"), pair("6", "5")], ..base() }).contains("twice"));
    assert!(refused(WorkRules {
        runs: RunRules { min_piece_secs: Some(10), max_piece_secs: Some(5), ..Default::default() },
        ..base()
    })
    .contains("piece"));
}
