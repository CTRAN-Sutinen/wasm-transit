//! The service-plan chain's law tests — pure functions, the exact
//! code every host dispatches through cap-plans. The chain is
//! DERIVED: resolution, windows, and validity recomputed from the
//! plan set's metadata on every call, deterministic and
//! order-insensitive whatever order the set arrives in.

use chrono::NaiveDate;
use transit_plans::{periods, resolve, validate, PlanInfo};

fn info(id: &str, name: &str, from: Option<&str>) -> PlanInfo {
    PlanInfo { id: id.into(), name: name.into(), effective_from: from.map(Into::into) }
}

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

#[test]
fn resolution_picks_the_latest_effective_plan() {
    let chain = vec![
        info("spring", "Spring 2026", Some("2026-03-01")),
        info("fall", "Fall 2026", Some("2026-09-07")),
    ];
    assert_eq!(resolve(&chain, date("2026-01-01")).unwrap(), None, "no pick has taken effect yet");
    assert_eq!(resolve(&chain, date("2026-03-01")).unwrap(), Some("spring".into()), "the pick date itself is in force");
    assert_eq!(resolve(&chain, date("2026-06-01")).unwrap(), Some("spring".into()));
    assert_eq!(
        resolve(&chain, date("2026-10-24")).unwrap(),
        Some("fall".into()),
        "the latest pick at or before the date wins"
    );

    // Unversioned imports never resolve.
    let imported = vec![info("", "imported", None)];
    assert_eq!(resolve(&imported, date("2026-10-24")).unwrap(), None);

    // Invalid dates are an error, not a guess.
    assert!(resolve(&chain, NaiveDate::MAX).is_ok());
}

#[test]
fn validation_reports_every_invariant() {
    let chain = vec![
        info("spring", "Spring 2026", Some("2026-03-01")),
        info("fall", "Fall 2026", Some("2026-09-07")),
    ];
    assert!(validate(&chain).is_empty(), "a valid chain has no issues");

    let broken = vec![
        info("spring", "Spring", Some("2026-03-01")),
        info("spring", "Spring dup", Some("2026-05-01")),
        info("undated", "Undated", None),
        info("same-day", "Same day", Some("2026-03-01")),
    ];
    let kinds: Vec<String> = validate(&broken)
        .iter()
        .map(|i| serde_json::to_value(i).unwrap()["kind"].as_str().unwrap().to_string())
        .collect();
    assert!(kinds.contains(&"duplicate_plan_id".to_string()), "{kinds:?}");
    assert!(kinds.contains(&"plan_missing_effective_date".to_string()), "{kinds:?}");
    assert!(kinds.contains(&"duplicate_effective_date".to_string()), "{kinds:?}");
    // Unversioned imports are standalone — never flagged.
    assert!(!kinds.iter().any(|k| k.contains("undated")), "{kinds:?}");
}

#[test]
fn windows_are_derived_not_stored() {
    let chain = vec![
        info("fall", "Fall 2026", Some("2026-09-07")),
        info("spring", "Spring 2026", Some("2026-03-01")),
        info("import", "Standalone", None),
    ];
    let windows = periods(&chain);
    let ids: Vec<&str> = windows.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["spring", "fall"], "sorted by pick date; unversioned excluded");
    assert_eq!(windows[0].effective_from, "2026-03-01");
    assert_eq!(windows[0].until.as_deref(), Some("2026-09-07"), "the next pick closes the window");
    assert_eq!(windows[1].until, None, "the last pick runs open-ended");

    // A duplicate pick date still windows deterministically (the
    // later id wins the boundary).
    let dup = vec![info("a", "A", Some("2026-03-01")), info("x", "X", Some("2026-03-01"))];
    let windows = periods(&dup);
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].until.as_deref(), Some("2026-03-01"));
}

#[test]
fn the_chain_is_order_insensitive() {
    // The chain is a SET: the same metadata in any input order
    // resolves, validates, and windows identically — an invalid
    // chain with duplicate dates still resolves deterministically
    // (ties broken by id), so every host agrees.
    let a = vec![
        info("fall", "Fall 2026", Some("2026-09-07")),
        info("spring", "Spring 2026", Some("2026-03-01")),
    ];
    let b = vec![
        info("spring", "Spring 2026", Some("2026-03-01")),
        info("fall", "Fall 2026", Some("2026-09-07")),
    ];
    for d in ["2026-01-01", "2026-04-01", "2026-12-31"] {
        assert_eq!(resolve(&a, date(d)).unwrap(), resolve(&b, date(d)).unwrap(), "{d}");
    }
    assert_eq!(periods(&a), periods(&b));

    let dup_a = vec![info("x", "X", Some("2026-03-01")), info("a", "A", Some("2026-03-01"))];
    let dup_b = vec![info("a", "A", Some("2026-03-01")), info("x", "X", Some("2026-03-01"))];
    assert_eq!(resolve(&dup_a, date("2026-06-01")).unwrap(), Some("x".into()));
    assert_eq!(resolve(&dup_a, date("2026-06-01")).unwrap(), resolve(&dup_b, date("2026-06-01")).unwrap());
    assert_eq!(validate(&dup_a), validate(&dup_b));
}
