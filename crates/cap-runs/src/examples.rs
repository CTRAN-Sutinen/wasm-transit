use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let rules = fixtures::ctran_2022_work_rules();
    let constraints = fixtures::fixture_scheduling_constraints().checked().expect("the fixture scheduling constraints pass");
    let loads = vec![
        fixtures::load_network(&net.derived),
        fixtures::load("load_work_rules", "work_rules", &rules),
        fixtures::load("load_scheduling_constraints", "scheduling_constraints", &constraints),
    ];
    vec![Example {
        bytes: None,
        kinds: vec![],
        op: "cut",
        loads,
        request: json!({
            "op": "cut",
            "network_hash": net.network_hash,
            "date": "2026-10-23",
            "work_rules_hash": cap_abi::canonical_hash(&rules).expect("the rules hash"),
            "scheduling_constraints_hash": cap_abi::canonical_hash(&constraints).expect("the constraints hash"),
        }),
    }]
}
