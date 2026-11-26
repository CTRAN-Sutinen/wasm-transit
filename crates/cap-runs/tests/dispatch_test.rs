//! The runs capability's dispatch: a cut over the fixture network's Friday,
//! its two steps, a hand-composed run judged, the drives to and from the
//! garage over the streets, and the refusals that name the load or the
//! piece a request lacks.

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
        streets_hash: None,
    })
    .map(|r| match r {
        Response::Cut { cut } => cut,
        _ => unreachable!("a cut answers a cut"),
    })
}

fn pieces(network: &str, rules: &str) -> Vec<transit_runs::Piece> {
    match dispatch(Request::Pieces {
        network_hash: network.into(),
        edits_hash: None,
        date: DATE.into(),
        work_rules_hash: rules.into(),
        scheduling_constraints_hash: None,
        streets_hash: None,
    })
    .unwrap()
    {
        Response::Pieces { pieces } => pieces.pieces,
        _ => unreachable!("pieces answer pieces"),
    }
}

fn judge(network: &str, rules: &str, runs: Vec<transit_runs::RunRef>) -> Result<transit_runs::RunSet, cap_abi::Refusal> {
    dispatch(Request::Judge {
        network_hash: network.into(),
        edits_hash: None,
        date: DATE.into(),
        work_rules_hash: rules.into(),
        scheduling_constraints_hash: None,
        streets_hash: None,
        runs,
    })
    .map(|r| match r {
        Response::Judge { judged } => judged,
        _ => unreachable!("a judge answers a judge"),
    })
}

#[test]
fn the_cuts_pieces_composed_are_the_cut() {
    let (network, rules) = register();
    let c = cut(&network, &rules).unwrap();
    let refs: Vec<transit_runs::PieceRef> = pieces(&network, &rules).iter().map(transit_runs::PieceRef::of).collect();
    let composed = match dispatch(Request::Compose {
        network_hash: network.clone(),
        edits_hash: None,
        date: DATE.into(),
        work_rules_hash: rules.clone(),
        scheduling_constraints_hash: None,
        streets_hash: None,
        pieces: refs,
    })
    .unwrap()
    {
        Response::Compose { composed } => composed,
        _ => unreachable!("compose answers compose"),
    };
    assert_eq!(composed.runs, c.runs);
    assert_eq!(composed.mix, c.mix);
}

#[test]
fn a_hand_composed_run_is_judged_and_an_unknown_piece_or_class_refused() {
    let (network, rules) = register();
    let p = pieces(&network, &rules);
    let run = |class: Option<&str>, pieces: Vec<transit_runs::PieceRef>| transit_runs::RunRef { id: "hand".into(), class: class.map(String::from), pieces };
    let set = judge(&network, &rules, vec![run(None, vec![transit_runs::PieceRef::of(&p[0])])]).unwrap();
    assert_eq!(set.runs.len(), 1);
    assert!(set.doubled.is_empty());
    assert!(!set.uncovered.is_empty() || p.len() == 1, "the other pieces' trips are in no run");

    let err = judge(&network, &rules, vec![run(Some("Z"), vec![transit_runs::PieceRef::of(&p[0])])]).unwrap_err();
    assert!(err.message.contains("no class Z"), "{err}");
    let mut bad = transit_runs::PieceRef::of(&p[0]);
    bad.first_trip = "nope".into();
    let err = judge(&network, &rules, vec![run(None, vec![bad])]).unwrap_err();
    assert!(err.message.contains("no trip nope"), "{err}");
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
        streets_hash: None,
    })
    .unwrap_err();
    assert!(err.message.contains("load_scheduling_constraints"), "{err}");
}

/// One street through the garage and every stop of the fixture network,
/// west to east, so each drive has a way.
fn street_through_every_stop(garage: (f64, f64)) -> transit_streets::Streets {
    let net = fixtures::network().derived_network().clone();
    let mut points: Vec<(f64, f64)> = net.stops.values().map(|s| (s.lon, s.lat)).chain(std::iter::once((garage.1, garage.0))).collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    let coords: Vec<serde_json::Value> = points.iter().map(|(lon, lat)| serde_json::json!([lon, lat])).collect();
    let layer = serde_json::json!({"type": "FeatureCollection", "features": [
        {"type": "Feature", "properties": {"name": "Main St"}, "geometry": {"type": "LineString", "coordinates": coords}}
    ]});
    transit_streets::geojson::import(&serde_json::to_vec(&layer).unwrap()).unwrap()
}

#[test]
fn a_cut_over_the_streets_drives_each_pull_out_and_in_and_reports_at_the_garage() {
    let (network, rules) = register();
    let constraints = fixtures::fixture_scheduling_constraints().checked().unwrap();
    let g = &constraints.garages[0];
    let streets = kind::<transit_streets::Streets>().register(street_through_every_stop((g.lat, g.lon))).unwrap().0;
    let constraints = kind::<transit_runs::SchedulingConstraints>().register(constraints).unwrap().0;
    let cut_with = |streets_hash: Option<String>| match dispatch(Request::Cut {
        network_hash: network.clone(),
        edits_hash: None,
        date: DATE.into(),
        work_rules_hash: rules.clone(),
        scheduling_constraints_hash: Some(constraints.clone()),
        streets_hash,
    })
    .unwrap()
    {
        Response::Cut { cut } => cut,
        _ => unreachable!("a cut answers a cut"),
    };
    let driven = cut_with(Some(streets));
    assert!(!driven.runs.is_empty());
    for r in &driven.runs {
        assert!(r.judged.deadhead_secs.is_some(), "run {} drives every pull: {:?}", r.id, r.judged.not_judged);
        for p in &r.pieces {
            assert_eq!(p.garage.as_deref(), (p.pull_out || p.pull_in).then_some("yard"));
            assert_eq!(p.deadhead_out_secs.is_some(), p.pull_out);
        }
        if r.pieces[0].pull_out {
            assert_eq!(r.judged.report.place.as_deref(), Some("yard"));
        }
    }
    let unknown = cut_with(None);
    assert!(unknown.runs.iter().all(|r| r.judged.deadhead_secs.is_none() || !r.pieces.iter().any(|p| p.pull_out || p.pull_in)));
    assert!(unknown.runs.iter().any(|r| r.judged.not_judged.iter().any(|f| f.rule.starts_with("the deadhead"))));
}
