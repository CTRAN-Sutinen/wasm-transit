use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    let payload = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/e2e/fixtures/sample-rt.pb"
    ))
    .expect("the rt fixture");
    let ctx = fixtures::rt_example_context().expect("the rt fixture context");
    let joined = || {
        vec![
            fixtures::load_network(&ctx.network),
            fixtures::load("load_boundary", "boundary", &ctx.boundary),
            fixtures::load("load_rt", "bytes", &payload),
        ]
    };
    let net = fixtures::network();
    let alerts = fixtures::fixture_alerts()
        .checked()
        .expect("the fixture alerts pass");
    let feed = transit_rt::encode_alerts(&net.network, &alerts, fixtures::ALERTS_NOW).bytes;
    vec![
        Example {
            bytes: Some((0, payload.clone())),
            kinds: vec![],
            op: "decode",
            loads: vec![],
            request: json!({ "op": "decode", "bytes": "<the raw GTFS-rt protobuf — bulk-bytes op 0>" }),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "trip_progress",
            loads: joined(),
            request: json!({ "op": "trip_progress", "network_hash": ctx.network_hash, "boundary_hash": ctx.boundary_hash, "rt_hash": ctx.rt_hash, "trip_id": "t10" }),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "live",
            loads: joined(),
            request: json!({ "op": "live", "network_hash": ctx.network_hash, "boundary_hash": ctx.boundary_hash, "rt_hash": ctx.rt_hash,
                             "window": { "late_secs": 60, "very_late_secs": 300 } }),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "late_trips",
            loads: joined(),
            request: json!({ "op": "late_trips", "network_hash": ctx.network_hash, "rt_hash": ctx.rt_hash, "min_delay_secs": 60, "vp_hash": null }),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "read_alerts",
            loads: vec![fixtures::load_network(&net.derived)],
            request: json!({ "op": "read_alerts", "network_hash": net.network_hash, "bytes": feed }),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "encode_alerts",
            loads: vec![
                fixtures::load_network(&net.derived),
                fixtures::load("load_service_alerts", "alerts", &alerts),
            ],
            request: json!({ "op": "encode_alerts", "network_hash": net.network_hash, "alerts_hash": cap_abi::canonical_hash(&alerts).expect("a hash"),
                             "timestamp": fixtures::ALERTS_NOW }),
        },
    ]
}
