//! Native dispatch tests for the authoring capability — the seam
//! without wasm, over the committed planner network. The laws (the
//! edit semantics themselves live in transit-network's edit_test):
//!
//! 1. the document identity is the canonical-serialization hash
//!    every capability returns;
//! 2. apply answers the next document REGISTERED under its own hash —
//!    changes chain by hash, and the last document stays addressable;
//! 3. issues and warnings ride along (the authoring loop in one op);
//! 4. a refused change, an unknown hash, and garbage are errors,
//!    never panics.

use cap_author::{dispatch, dispatch_bytes, dispatch_json, Request, Response};
use sha2::Digest;
use transit_network::{Network, NetworkEdit, NetworkWarning, PlanMeta};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap()
}

fn load(network: Network) -> String {
    match dispatch(Request::LoadNetwork { network }).unwrap() {
        Response::LoadNetwork { network_hash } => network_hash,
        _ => panic!("load_network"),
    }
}

fn apply(network_hash: &str, edits: Vec<NetworkEdit>) -> Result<Response, String> {
    dispatch(Request::Apply { network_hash: network_hash.into(), edits })
}

#[test]
fn the_identity_is_the_canonical_hash() {
    let net = network();
    let expected = format!("sha256:{:x}", sha2::Sha256::digest(serde_json::to_string(&net).unwrap().as_bytes()));
    assert_eq!(load(net), expected);
}

#[test]
fn changes_chain_by_hash_and_report_the_result() {
    let base = load(network());
    // Fork into a pick dated after the calendar ends: applied, and
    // the warning says the pick governs a period with no service.
    let Response::Apply { network_hash: pick, network, issues, warnings } = apply(
        &base,
        vec![NetworkEdit::SetPlan {
            plan: PlanMeta {
                id: "spring".into(),
                name: "Spring 2027".into(),
                effective_from: chrono::NaiveDate::from_ymd_opt(2027, 1, 10),
            },
        }],
    )
    .unwrap() else {
        panic!("apply")
    };
    assert_ne!(pick, base, "a change is a new document");
    assert_eq!(network.plan.id, "spring");
    assert!(issues.is_empty());
    // (The 25:30 trip was never in block 400: unassigned, warned.)
    let unassigned = NetworkWarning::UnassignedTrips { schedule: "10-weekday".into(), trips: 1 };
    assert_eq!(
        warnings,
        vec![
            unassigned.clone(),
            NetworkWarning::NoServiceInPlanPeriod { plan: "spring".into(), effective_from: "2027-01-10".into() },
        ]
    );

    // The next change chains on the answered hash (no reload): extend
    // the weekday calendar into 2027 and the plan-period warning clears.
    let mut weekday = network.service_types["weekday"].clone();
    weekday.rule.ranges[0].end = chrono::NaiveDate::from_ymd_opt(2027, 12, 31).unwrap();
    let Response::Apply { warnings, .. } =
        apply(&pick, vec![NetworkEdit::UpsertServiceType { id: "weekday".into(), service_type: weekday }]).unwrap()
    else {
        panic!("apply")
    };
    assert_eq!(warnings, vec![unassigned], "the plan-period warning clears");

    // The last document stays addressable: a change from the base
    // again lands on the same identity as before (idempotent).
    let Response::Apply { network_hash: again, .. } = apply(
        &base,
        vec![NetworkEdit::SetPlan {
            plan: PlanMeta {
                id: "spring".into(),
                name: "Spring 2027".into(),
                effective_from: chrono::NaiveDate::from_ymd_opt(2027, 1, 10),
            },
        }],
    )
    .unwrap() else {
        panic!("apply")
    };
    assert_eq!(again, pick, "the same change of the same document is the same document");
}

#[test]
fn refusals_and_garbage_are_errors_not_panics() {
    let base = load(network());
    let err = apply(&base, vec![NetworkEdit::RemoveStop { id: "A".into() }]).unwrap_err();
    assert!(err.contains("edit 0") && err.contains("visited by pattern"), "{err}");
    let err = apply("sha256:missing", vec![]).unwrap_err();
    assert!(err.contains("unknown network"), "{err}");

    // The JSON seam: tagged edits in, the outcome wrapper out.
    let out = dispatch_json(&format!(
        r#"{{"op":"apply","network_hash":"{base}","edits":[{{"edit":"set_name","name":"Renamed"}}]}}"#
    ));
    assert!(out.contains(r#""op":"apply""#) && out.contains(r#""name":"Renamed""#), "{out}");
    assert!(dispatch_json("not json").contains("invalid request"));
    assert!(dispatch_bytes(0, b"x").contains("unknown bulk op"));
}
