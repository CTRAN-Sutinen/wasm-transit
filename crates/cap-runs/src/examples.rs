use cap_abi::manifest::Example;
use serde_json::json;

const DATE: &str = "2026-10-23";

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let rules = fixtures::ctran_2022_work_rules();
    let constraints = fixtures::fixture_scheduling_constraints()
        .checked()
        .expect("the fixture scheduling constraints pass");
    let loads = || {
        vec![
            fixtures::load_network(&net.derived),
            fixtures::load("load_work_rules", "work_rules", &rules),
            fixtures::load(
                "load_scheduling_constraints",
                "scheduling_constraints",
                &constraints,
            ),
        ]
    };
    let rules_hash = cap_abi::canonical_hash(&rules).expect("the rules hash");
    let constraints_hash = cap_abi::canonical_hash(&constraints).expect("the constraints hash");

    let network = net.derived_network();
    let on = transit_dispatch::day_date(DATE).expect("the date reads");
    let day = transit_dispatch::day_of(network, &[], on).expect("the Friday materializes");
    let places = transit_runs::NetworkPlaces::new(network, &day.trips);
    let pieces = transit_runs::cut_pieces(
        &transit_runs::DayBlocks::new(network, &day.trips, Some(&constraints), None),
        &rules,
        Some(&constraints),
        DATE,
        on.weekday().into(),
        &places,
    )
    .pieces;
    let refs: Vec<transit_runs::PieceRef> = pieces.iter().map(transit_runs::PieceRef::of).collect();
    // A run a planner composed by hand: the first piece, and the first later
    // piece that starts after it ends.
    let first = &pieces[0];
    let later = pieces
        .iter()
        .find(|p| p.start_secs() >= first.end_secs())
        .unwrap_or(first);
    let hand = json!([{"id": "hand-1", "pieces": [transit_runs::PieceRef::of(first), transit_runs::PieceRef::of(later)]}]);

    let request = |op: &str| {
        json!({
            "op": op,
            "network_hash": net.network_hash,
            "date": DATE,
            "work_rules_hash": rules_hash,
        })
    };
    let with = |op: &str, field: &str, value: serde_json::Value| {
        let mut r = request(op);
        r[field] = value;
        r
    };
    let example = |op: &'static str, request: serde_json::Value| Example {
        bytes: None,
        kinds: vec![],
        op,
        loads: loads(),
        request,
    };
    vec![
        example(
            "cut",
            with(
                "cut",
                "scheduling_constraints_hash",
                json!(constraints_hash),
            ),
        ),
        example(
            "pieces",
            with(
                "pieces",
                "scheduling_constraints_hash",
                json!(constraints_hash),
            ),
        ),
        example("compose", with("compose", "pieces", json!(refs))),
        example("judge", {
            let mut r = with("judge", "runs", hand);
            r["scheduling_constraints_hash"] = json!(constraints_hash);
            r
        }),
    ]
}
