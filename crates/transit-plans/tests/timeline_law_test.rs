//! The timeline over device plans and the published set. Rows are
//! every device plan, then each published pick the device does not hold. A
//! retired pick is no row. A device plan is published when its id is a
//! published pick's or it is `served`.

use transit_date::Date;
use transit_plans::{timeline, DevicePlan, PlanIssue, PublishedPlans};

fn day(s: &str) -> Date {
    Date::parse(s).unwrap()
}

fn set(picks: &[(&str, &str, &str, &str)]) -> PublishedPlans {
    let plans: Vec<_> = picks
        .iter()
        .map(|(id, name, from, status)| serde_json::json!({"id": id, "name": name, "effective_from": from, "network_hash": "sha256:00", "status": status}))
        .collect();
    serde_json::from_value(serde_json::json!({"kind": "plans", "schema_version": 1, "plans": plans})).expect("a published set")
}

fn device(key: &str, id: &str, name: &str, from: Option<&str>, served: bool) -> DevicePlan {
    DevicePlan { key: key.into(), id: id.into(), name: name.into(), effective_from: from.map(Into::into), served }
}

#[test]
fn rows_are_the_device_plans_then_the_picks_it_does_not_hold() {
    let published = set(&[("fall", "Fall", "2026-09-07", "published"), ("winter", "Winter", "2027-01-04", "published")]);
    let plans = [device("plan-a", "fall", "My fall", Some("2026-09-07"), false), device("plan-b", "draft", "Draft", Some("2027-03-01"), false)];
    let t = timeline(&plans, Some(&published), day("2026-10-01")).unwrap();
    let rows: Vec<_> = t.rows.iter().map(|r| (r.key.as_deref(), r.id.as_str(), r.published, r.loaded)).collect();
    assert_eq!(rows, vec![(Some("plan-a"), "fall", true, true), (Some("plan-b"), "draft", false, true), (None, "winter", true, false)]);
    assert_eq!(t.rows[0].name, "My fall", "a plan on the device keeps its own name");
    assert_eq!(t.rows[2].name, "Winter");
    assert_eq!(t.rows[2].effective_from.as_deref(), Some("2027-01-04"));
}

#[test]
fn a_retired_pick_is_no_row_and_publishes_nothing() {
    let published = set(&[("old", "Old", "2026-09-07", "retired"), ("fall", "Fall", "2026-10-19", "published")]);
    let plans = [device("plan-a", "old", "Old", Some("2026-09-07"), false)];
    let t = timeline(&plans, Some(&published), day("2026-11-01")).unwrap();
    assert!(!t.rows[0].published, "a retired pick is not in production");
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[1].id, "fall");
    assert_eq!(t.in_force.as_deref(), Some("fall"));
}

#[test]
fn the_deployments_feed_is_published_whatever_its_id() {
    let plans = [device("served:x", "feed", "Feed", Some("2026-01-01"), true), device("plan-a", "mine", "Mine", None, false)];
    let t = timeline(&plans, None, day("2026-11-01")).unwrap();
    assert_eq!(t.rows.iter().map(|r| r.published).collect::<Vec<_>>(), vec![true, false]);
}

#[test]
fn the_chain_covers_every_row_with_an_id() {
    let published = set(&[("fall", "Fall", "2026-10-19", "published"), ("winter", "Winter", "2027-01-04", "published")]);
    let plans = [device("plan-a", "fall", "Fall", Some("2026-10-19"), false), device("plan-b", "", "Import", None, false)];
    let t = timeline(&plans, Some(&published), day("2026-12-01")).unwrap();
    let periods: Vec<_> = t.periods.iter().map(|p| (p.id.as_str(), p.until.as_deref())).collect();
    assert_eq!(periods, vec![("fall", Some("2027-01-04")), ("winter", None)]);
    assert_eq!(t.in_force.as_deref(), Some("fall"));
    assert!(t.issues.is_empty());
    let clash = [device("a", "p", "P", Some("2026-10-19"), false), device("b", "q", "Q", Some("2026-10-19"), false)];
    let t = timeline(&clash, None, day("2026-12-01")).unwrap();
    assert!(t.issues.iter().any(|i| matches!(i, PlanIssue::DuplicateEffectiveDate { .. })));
    assert_eq!(t.in_force, None, "no published set: nothing is in force");
}

#[test]
fn nothing_is_in_force_before_the_first_pick() {
    let published = set(&[("fall", "Fall", "2026-10-19", "published")]);
    let t = timeline(&[], Some(&published), day("2026-10-01")).unwrap();
    assert_eq!(t.in_force, None);
    assert_eq!(t.rows.len(), 1);
}
