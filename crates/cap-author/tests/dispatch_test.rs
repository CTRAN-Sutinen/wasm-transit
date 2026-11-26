//! Native tests of `apply` and `stamp_pick` over the committed planner
//! network (cap-author/OPS.md). The edit semantics are in transit-network's
//! `edit_test`.

use cap_author::{dispatch, dispatch_bytes, dispatch_json, Request, Response};
use sha2::Digest;
use transit_network::{Network, NetworkEdit, NetworkWarning, PlanMeta};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap()
}

fn load(network: Network) -> String {
    cap_abi::kernel::kind::<transit_network::Network>().register(network).unwrap().0
}

fn apply(network_hash: &str, edits: Vec<NetworkEdit>) -> Result<Response, cap_abi::Refusal> {
    dispatch(Request::Apply { network_hash: network_hash.into(), edits, work_rules_hash: None })
}

#[test]
fn the_identity_is_the_canonical_hash() {
    let net = network();
    let expected = format!("sha256:{:x}", sha2::Sha256::digest(cap_abi::canonical_bytes(&net).unwrap().as_bytes()));
    assert_eq!(load(net), expected);
}

#[test]
fn changes_chain_by_hash_and_report_the_result() {
    let base = load(network());
    let Response::Apply { network_hash: pick, network, issues, warnings, created, touched } = apply(
        &base,
        vec![NetworkEdit::SetPlan {
            plan: PlanMeta {
                id: "spring".into(),
                name: "Spring 2027".into(),
                effective_from: transit_date::Date::from_ymd_opt(2027, 1, 10),
                extra: Default::default(),
            },
            through: None,
        }],
    )
    .unwrap() else {
        panic!("apply")
    };
    assert_ne!(pick, base, "a change is a new document");
    assert!(created.is_empty(), "a plan frame creates nothing");
    assert!(touched.is_empty());
    assert_eq!(network.plan.id, "spring");
    assert!(issues.is_empty());
    let unassigned = NetworkWarning::UnassignedTrips { pattern_trips: "10-weekday".into(), trips: 1 };
    assert_eq!(
        warnings,
        vec![
            unassigned.clone(),
            NetworkWarning::NoServiceInPlanPeriod { plan_name: "Spring 2027".into(), effective_from: "2027-01-10".into() },
        ]
    );

    // A fork's calendar is its period: the ranges outside it go.
    assert!(network.service_types["weekday"].rule.ranges.is_empty());
    let Response::Apply { warnings, .. } = apply(
        &pick,
        vec![NetworkEdit::AddServiceRange {
            id: "weekday".into(),
            start: Some("2027-01-10".into()),
            end: Some("2027-12-31".into()),
        }],
    )
    .unwrap()
    else {
        panic!("apply")
    };
    assert_eq!(warnings, vec![unassigned], "the plan-period warning clears");

    let Response::Apply { network_hash: again, .. } = apply(
        &base,
        vec![NetworkEdit::SetPlan {
            plan: PlanMeta {
                id: "spring".into(),
                name: "Spring 2027".into(),
                effective_from: transit_date::Date::from_ymd_opt(2027, 1, 10),
                extra: Default::default(),
            },
            through: None,
        }],
    )
    .unwrap() else {
        panic!("apply")
    };
    assert_eq!(again, pick, "the same change of the same document is the same document");

    // A fork to a period end carries each weekly type to that end.
    let Response::Apply { network, warnings, .. } = apply(
        &base,
        vec![NetworkEdit::SetPlan {
            plan: PlanMeta {
                id: "spring".into(),
                name: "Spring 2027".into(),
                effective_from: transit_date::Date::from_ymd_opt(2027, 1, 10),
                extra: Default::default(),
            },
            through: Some("2027-05-29".into()),
        }],
    )
    .unwrap() else {
        panic!("apply")
    };
    let monday = transit_date::Date::from_ymd_opt(2027, 1, 11).unwrap();
    assert!(network.service_types["weekday"].rule.runs_on(monday));
    assert_eq!(warnings, vec![NetworkWarning::UnassignedTrips { pattern_trips: "10-weekday".into(), trips: 1 }]);
}

/// A redraw reports the patterns it touched.
#[test]
fn a_redraw_reports_the_patterns_it_touched() {
    let base = load(network());
    let edit: NetworkEdit = serde_json::from_value(serde_json::json!({
        "edit": "draw_alignment", "pattern": "10-inbound", "redraw": "one",
        "points": [{"lat": 45.5, "lon": -73.5}, {"lat": 45.51, "lon": -73.51}]
    }))
    .unwrap();
    let Response::Apply { network, created, touched, .. } = apply(&base, vec![edit]).unwrap() else { panic!("apply") };
    let new_line = network.patterns["10-inbound"].alignment.clone().unwrap();
    assert_eq!(created, vec![transit_network::CreatedEntity { kind: "alignment".into(), id: new_line }]);
    assert_eq!(touched, vec![transit_network::TouchedEntity { kind: "pattern".into(), id: "10-inbound".into() }]);
}

#[test]
fn refusals_and_garbage_are_errors_not_panics() {
    let base = load(network());
    let err = apply(&base, vec![NetworkEdit::RemoveStop { id: "A".into() }]).unwrap_err();
    assert!(err.message.contains("edit 0") && err.message.contains("visited by pattern"), "{err}");
    let err = apply("sha256:missing", vec![]).unwrap_err();
    assert!(err.message.contains("unknown network"), "{err}");

    let out = dispatch_json(&format!(
        r#"{{"op":"apply","network_hash":"{base}","edits":[{{"edit":"set_name","name":"Renamed"}}]}}"#
    ));
    assert!(out.contains(r#""op":"apply""#) && out.contains(r#""name":"Renamed""#), "{out}");
    assert!(dispatch_json("not json").contains("invalid request"));
    assert!(dispatch_bytes(0, b"x").contains("unknown bulk op"));
}

#[test]
fn gestures_report_what_they_created() {
    let base = load(network());
    let Response::Apply { network_hash, created, .. } =
        apply(&base, vec![NetworkEdit::AddStop { id: None, lat: None, lon: None }]).unwrap()
    else {
        panic!("apply")
    };
    assert_eq!(created.len(), 1);
    assert_eq!((created[0].kind.as_str(), created[0].id.as_str()), ("stop", "s1"));
    let Response::Apply { network, .. } = apply(
        &network_hash,
        vec![NetworkEdit::UpdateStop { id: "s1".into(), name: Some("Depot".into()), lat: None, lon: None, transfer: None }],
    )
    .unwrap() else {
        panic!("apply")
    };
    assert_eq!(network.stops["s1"].name, "Depot");
    let err = apply(&network_hash, vec![NetworkEdit::RemoveTrip { trip: "ghost".into() }]).unwrap_err();
    assert!(err.message.contains("edit 0") && err.message.contains("unknown trip"), "{err}");
}

#[test]
fn the_kernel_bulk_op_registers_the_same_identity_as_load_network() {
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_author::invoke(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_author::invoke_bytes(<transit_network::Network as cap_abi::kernel::Load>::CODE, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_author::invoke_bytes(<transit_network::Network as cap_abi::kernel::Load>::CODE, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}

/// Work rules win over the terminal's floor.
#[test]
fn work_rules_judge_the_waits_of_the_next_document() {
    let base = load(network());
    let issues = |work_rules_hash: Option<String>| {
        match dispatch(Request::Apply { network_hash: base.clone(), edits: vec![], work_rules_hash }).unwrap() {
            Response::Apply { issues, .. } => issues,
            _ => unreachable!("apply answers apply"),
        }
    };
    assert_eq!(issues(None), vec![], "block 400 keeps the floor at its terminal");
    let rules = transit_runs::WorkRules {
        schema_version: Default::default(),
        recovery: vec![transit_runs::RecoveryRule { route: None, service_type: None, min_secs: Some(86_400), pct_of_run: None, extra: Default::default() }],
        ..Default::default()
    };
    let loaded = cap_abi::kernel::invoke(
        &[&cap_author::CAPABILITY],
        &serde_json::json!({ "op": "load_work_rules", "work_rules": rules }).to_string(),
    );
    let hash = serde_json::from_str::<serde_json::Value>(&loaded).unwrap()["ok"]["work_rules_hash"].as_str().unwrap().to_string();
    let ruled = issues(Some(hash));
    assert!(
        matches!(&ruled[..], [transit_network::NetworkError::LayoverInfeasible { block, required_secs: 86_400, required_by: transit_network::RecoverySource::Policy, .. }] if block == "400"),
        "{ruled:?}"
    );
    let err = dispatch(Request::Apply { network_hash: base, edits: vec![], work_rules_hash: Some("sha256:nope".into()) }).unwrap_err();
    assert!(err.message.contains("load_work_rules"), "{err}");
}

/// A feed is stamped as a pick once.
#[test]
fn stamp_pick_stamps_a_feed_once() {
    let base = load(network());
    let Response::StampPick { network_hash, network, stamped, .. } = dispatch(Request::StampPick { network_hash: base.clone(), name: "Fall".into(), id: "fall".into() }).unwrap() else { panic!("stamp_pick") };
    assert!(stamped && network.plan.id == "fall" && network.plan.effective_from.is_some());
    let Response::StampPick { network_hash: again, stamped, .. } = dispatch(Request::StampPick { network_hash, name: "Other".into(), id: "other".into() }).unwrap() else { panic!("stamp_pick") };
    assert!(!stamped, "a pick is not stamped again");
    let _ = again;
}
