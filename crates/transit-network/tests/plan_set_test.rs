//! A stored plan set read back (docs/FORMATS.md, "Versions"): the
//! tagged set in this version answers its plans as stored, a bare map
//! of plans by store id (the shape written before the set had its tag)
//! reads as the set, and another kind or version is refused, naming
//! both. A page never writes over a set it refuses.

use serde_json::json;
use transit_network::plan_set::stored_plans;

#[test]
fn a_tagged_set_answers_its_plans_as_stored() {
    let plan = json!({"kind": "network", "schema_version": 1, "plan": {"id": "a"}});
    let plans = stored_plans(json!({"kind": "plan_set", "schema_version": 1, "plans": {"p1": plan}})).unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans["p1"], plan, "each plan as stored: its own version is read on use");
}

#[test]
fn a_set_from_before_its_tag_is_a_bare_map_of_plans() {
    let plans = stored_plans(json!({"p1": {"plan": {"id": ""}}, "p2": {"plan": {"id": "b"}}})).unwrap();
    assert_eq!(plans.keys().collect::<Vec<_>>(), ["p1", "p2"]);
}

#[test]
fn an_untagged_version_is_v1() {
    assert!(stored_plans(json!({"kind": "plan_set", "plans": {}})).unwrap().is_empty());
}

#[test]
fn another_kind_or_a_newer_version_is_refused_naming_both() {
    let newer = stored_plans(json!({"kind": "plan_set", "schema_version": 2, "plans": {}})).unwrap_err();
    assert!(newer.contains("plan_set v2") && newer.contains("reads v1"), "{newer}");
    let other = stored_plans(json!({"kind": "watchlist", "schema_version": 2, "stops": []})).unwrap_err();
    assert!(other.contains("watchlist") && other.contains("plan set"), "{other}");
    assert!(stored_plans(json!([])).is_err(), "a set is a JSON object");
    assert!(stored_plans(json!({"kind": "plan_set", "schema_version": 1})).is_err(), "a tagged set has its plans");
}
