//! The runs capability's dispatch: a cut over the fixture network's Friday,
//! and the refusals that name the load a request needs.

use cap_abi::kernel::kind;
use cap_runs::{dispatch, Request, Response};

const DATE: &str = "2026-10-23";

fn register() -> (String, String) {
    let network = kind::<transit_network::Network>().register(fixtures::network().derived_network().clone()).unwrap().0;
    let rules = kind::<transit_runs::WorkRules>().register(fixtures::ctran_2022_work_rules()).unwrap().0;
    (network, rules)
}

fn cut(network_hash: &str, work_rules_hash: &str) -> Result<transit_runs::Cut, cap_abi::Refusal> {
    dispatch(Request::Cut {
        network_hash: network_hash.into(),
        edits_hash: None,
        date: DATE.into(),
        work_rules_hash: work_rules_hash.into(),
        scheduling_constraints_hash: None,
    })
    .map(|r| match r {
        Response::Cut { cut } => cut,
    })
}

#[test]
fn the_cut_places_every_blocked_trip_of_the_friday_once() {
    let (network, rules) = register();
    let c = cut(&network, &rules).unwrap();
    let net = fixtures::network().derived_network().clone();
    let blocked: std::collections::BTreeSet<&str> = net.blocks.values().flat_map(|b| b.trips.iter().map(|t| t.trip.as_str())).collect();
    let placed: Vec<&str> = c.runs.iter().flat_map(|r| &r.pieces).flat_map(|p| &p.trips).map(|t| t.trip.as_str()).collect();
    assert!(!c.runs.is_empty(), "the fixture's blocks run on the Friday");
    assert!(placed.iter().all(|t| blocked.contains(t)));
    let unique: std::collections::BTreeSet<&str> = placed.iter().copied().collect();
    assert_eq!(unique.len(), placed.len(), "no trip twice");
    assert_eq!(c.mix.len(), 3, "the C-TRAN rules' three mix rules are reported");
    assert!(c.warnings.iter().any(|w| w.contains("outside the period")), "2026 is past the 2022–2025 agreement: {:?}", c.warnings);
}

#[test]
fn a_cut_names_the_load_it_lacks() {
    let (network, rules) = register();
    let err = cut(&network, "sha256:nope").unwrap_err();
    assert!(err.message.contains("load_work_rules"), "{err}");
    let err = cut("sha256:nope", &rules).unwrap_err();
    assert!(err.message.contains("load_network"), "{err}");
    let err = dispatch(Request::Cut {
        network_hash: network,
        edits_hash: None,
        date: DATE.into(),
        work_rules_hash: rules,
        scheduling_constraints_hash: Some("sha256:nope".into()),
    })
    .unwrap_err();
    assert!(err.message.contains("load_scheduling_constraints"), "{err}");
}
