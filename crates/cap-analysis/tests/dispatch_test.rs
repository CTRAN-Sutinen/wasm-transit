//! Native dispatch tests for the analysis capability — the seam
//! without wasm, over a REAL document: the network is built by the
//! monolith's own import (gtfs_to_network), so the capability is
//! exercised on exactly the document a planner would author or
//! derive. The content-hash registry gets explicit laws, same as the
//! other capabilities: same document, same hash, one entry.

use std::io::Write;

use proptest::prelude::*;

#[derive(Clone, Debug)]
struct GenTrip {
    route: u8,
    service: u8,
    block: Option<u8>,
    start_slot: u16,
    stops: Vec<u8>,
    leg_min: u8,
}

fn gen_trip() -> impl Strategy<Value = GenTrip> {
    (
        0u8..2,
        0u8..2,
        proptest::option::of(0u8..2),
        0u16..96,
        proptest::collection::vec(0u8..6, 2..=4),
        1u8..30,
    )
        .prop_map(|(route, service, block, start_slot, stops, leg_min)| {
            let mut seq: Vec<u8> = stops
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if seq.len() < 2 {
                seq = vec![0, 1];
            }
            GenTrip { route, service, block, start_slot, stops: seq, leg_min }
        })
}

fn gen_feed(trips: &[GenTrip]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };

    add(
        &mut w,
        "agency.txt",
        "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n",
    );
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n0,1,0,3\n1,1,1,3\n");
    let mut stops = String::from("stop_id,stop_name,stop_lat,stop_lon");
    for i in 0..6u8 {
        stops.push_str(&format!("\ns{i},Stop {i},45.50{},-73.50{}", i, i));
    }
    add(&mut w, "stops.txt", &stops);

    let mut trips_txt = String::from("route_id,service_id,trip_id,block_id");
    for (i, t) in trips.iter().enumerate() {
        trips_txt.push_str(&format!(
            "\n{},{},{},{}",
            t.route,
            t.service,
            format!("t{i}"),
            t.block.map(|b| b.to_string()).unwrap_or_default()
        ));
    }
    add(&mut w, "trips.txt", &trips_txt);

    let mut stop_times = String::from("trip_id,arrival_time,departure_time,stop_id,stop_sequence");
    for (i, t) in trips.iter().enumerate() {
        let id = format!("t{i}");
        let start = (t.start_slot as u32) * 600;
        for (j, &s) in t.stops.iter().enumerate() {
            let secs = start + (j as u32) * (t.leg_min as u32) * 60;
            let hhmmss = format!("{:02}:{:02}:00", secs / 3600, (secs % 3600) / 60);
            stop_times.push_str(&format!("\n{},{},{},s{},{}", id, hhmmss, hhmmss, s, j + 1));
        }
    }
    add(&mut w, "stop_times.txt", &stop_times);
    add(
        &mut w,
        "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n0,1,1,1,1,1,0,0,20260101,20261231\n1,0,0,0,0,0,1,1,20260101,20261231\n",
    );
    w.finish().unwrap().into_inner()
}

/// A REAL network document: the monolith's own inference over a
/// generated feed — the document a planner would derive.
fn network(trips: &[GenTrip]) -> transit_network::Network {
    let gtfs = transit_core::Gtfs::from_zip_bytes(&gen_feed(trips)).expect("parse");
    transit_codec::gtfs_to_network(&gtfs).network
}

fn layer_geojson() -> Vec<u8> {
    format!(
        "{{\"type\":\"FeatureCollection\",\"features\":[{}]}}",
        (0..6)
            .map(|i| format!(
                "{{\"type\":\"Feature\",\"properties\":{{\"pop\":{}}},\"geometry\":{{\"type\":\"Point\",\"coordinates\":[-73.50{},45.50{}]}}}}",
                100 * (i as u64 + 1), i, i
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
    .into_bytes()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// A real derived document loads, registers idempotently, and
    /// answers stats — the capability speaks the planner's own data.
    #[test]
    fn derived_documents_load_and_answer(
        trips in proptest::collection::vec(gen_trip(), 1..=14),
    ) {
        // The generator can produce weekend-only documents; pin one
        // trip to weekday service so the Monday probe has trips.
        let mut weekday_trips = trips;
        if let Some(first) = weekday_trips.first_mut() {
            first.service = 0;
        }
        let net = network(&weekday_trips);

        let first = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net.clone() })
            .expect("a valid document loads");
        let cap_analysis::Response::LoadNetwork { network_hash } = first else { panic!("load") };

        // Idempotence: the same logical document is one registry entry.
        let again = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net }).unwrap();
        let cap_analysis::Response::LoadNetwork { network_hash: same } = again else { panic!("reload") };
        prop_assert_eq!(&network_hash, &same);
        prop_assert!(network_hash.starts_with("sha256:"));

        // A weekday date: the document's service 0 runs (Mon 2026-01-05).
        let stats = cap_analysis::dispatch(cap_analysis::Request::NetworkStats {
            network_hash: network_hash.clone(),
            date: "2026-01-05".into(),
            include_routes: true,
            include_stops: true,
            include_service_types: true,
        })
        .expect("stats");
        let cap_analysis::Response::NetworkStats { stats } = stats else { panic!("stats") };
        prop_assert!(stats.trips > 0, "the imported document has weekday trips");
    }

    /// The design axis and coverage answer over hash-addressed
    /// documents — never per-op document serialization.
    #[test]
    fn overview_and_coverage_answer_by_hash(
        trips in proptest::collection::vec(gen_trip(), 1..=14),
    ) {
        let net = network(&trips);
        let cap_analysis::Response::LoadNetwork { network_hash } =
            cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net })
                .expect("load")
        else { panic!("load") };
        let layer = cap_analysis::dispatch(cap_analysis::Request::LoadPopulationLayer {
            bytes: layer_geojson(),
            pop_field: Some("pop".into()),
        })
        .expect("layer");
        let cap_analysis::Response::LoadPopulationLayer { layer_hash, points } = layer else { panic!("layer") };
        prop_assert_eq!(points, 6);

        let overview = cap_analysis::dispatch(cap_analysis::Request::ServiceTypesOverview {
            network_hash: network_hash.clone(),
        })
        .expect("overview");
        let cap_analysis::Response::ServiceTypesOverview { service_types } = overview else { panic!("ov") };
        prop_assert!(!service_types.is_empty());

        let coverage = cap_analysis::dispatch(cap_analysis::Request::TypeCoverage {
            network_hash,
            layer_hash,
            radius_m: 500.0,
            frequent_secs: 3600,
            min_departures: 2,
        })
        .expect("coverage");
        let cap_analysis::Response::TypeCoverage { coverage } = coverage else { panic!("cov") };
        prop_assert!(!coverage.is_empty());
        for entry in &coverage {
            prop_assert!(entry.served_pct >= 0.0 && entry.served_pct <= 100.0);
        }
    }
}

#[test]
fn unknown_hashes_are_errors_not_panics() {
    let err = cap_analysis::dispatch(cap_analysis::Request::NetworkStats {
        network_hash: "sha256:missing".into(),
        date: "2026-01-05".into(),
        include_routes: true,
        include_stops: true,
        include_service_types: true,
    })
    .expect_err("unknown hash");
    assert!(err.contains("unknown network"));
}

#[test]
fn json_seam_round_trips_the_tagged_form() {
    let request = serde_json::json!({
        "op": "load_network",
        "network": serde_json::to_value(network(&[GenTrip {
            route: 0, service: 0, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5,
        }])).unwrap(),
    });
    let loaded: serde_json::Value =
        serde_json::from_str(&cap_analysis::dispatch_json(&request.to_string())).unwrap();
    assert!(loaded["ok"]["network_hash"].as_str().unwrap().starts_with("sha256:"));
    let hash = loaded["ok"]["network_hash"].as_str().unwrap();

    let stats: serde_json::Value = serde_json::from_str(&cap_analysis::dispatch_json(&serde_json::json!({
        "op": "network_stats", "network_hash": hash, "date": "2026-01-05",
    }).to_string())).unwrap();
    assert!(stats["ok"]["stats"]["trips"].as_u64().unwrap() >= 1);
    // Omitted flags default TRUE (the UI's shape is unchanged).
    assert!(stats["ok"]["stats"]["routes"].as_array().unwrap().len() >= 1);

    let bad: serde_json::Value =
        serde_json::from_str(&cap_analysis::dispatch_json(r#"{"op": "nope"}"#)).unwrap();
    assert!(bad["error"].as_str().unwrap().contains("invalid request"));
}

/// The query-shaping law (the founding dogfood session's 199 KiB
/// answer to a five-number question): `include_routes` /
/// `include_stops` false empty the corresponding lists while every
/// summary number stays — the day-shape question answers in under a
/// KiB, and the defaults keep the UI's full payload.
#[test]
fn include_flags_shape_the_stats_response() {
    let net = network(&[GenTrip {
        route: 0, service: 0, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5,
    }]);
    let loaded = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net })
        .expect("load");
    let cap_analysis::Response::LoadNetwork { network_hash } = loaded else { panic!("load") };

    let shaped = cap_analysis::dispatch(cap_analysis::Request::NetworkStats {
        network_hash: network_hash.clone(),
        date: "2026-01-05".into(),
        include_routes: false,
        include_stops: false,
        include_service_types: false,
    })
    .expect("stats");
    let cap_analysis::Response::NetworkStats { stats: lean } = shaped else { panic!("stats") };

    let full = cap_analysis::dispatch(cap_analysis::Request::NetworkStats {
        network_hash,
        date: "2026-01-05".into(),
        include_routes: true,
        include_stops: true,
        include_service_types: true,
    })
    .expect("stats");
    let cap_analysis::Response::NetworkStats { stats: rich } = full else { panic!("stats") };

    assert!(lean.routes.is_empty() && lean.stops.is_empty(), "the lists empty on request");
    assert!(lean.service_types.is_empty(), "the design axis empties on request too");
    assert!(!rich.routes.is_empty() && !rich.stops.is_empty(), "defaults keep the UI's payload");
    // Every summary number is identical — shaping drops bulk, never
    // meaning.
    assert_eq!(lean.trips, rich.trips);
    assert_eq!(lean.active_routes, rich.active_routes);
    assert_eq!(lean.active_stops, rich.active_stops);
    assert_eq!(lean.revenue_hours, rich.revenue_hours);
    assert_eq!(lean.transfer_stops, rich.transfer_stops);
    // And the wire shows it: the lean answer is smaller.
    let lean_json = serde_json::to_string(&lean).unwrap();
    let rich_json = serde_json::to_string(&rich).unwrap();
    assert!(lean_json.len() < rich_json.len(), "lean {} vs rich {}", lean_json.len(), rich_json.len());
}
