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
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n0,1,1,1,1,1,0,0,20260101,20261231\n1,0,0,0,0,0,1,1,20260101,20261231\n2,1,0,1,0,0,0,0,20260101,20261231\n3,1,1,1,1,1,0,0,20260301,20260601\n",
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
        let cap_analysis::Response::LoadPopulationLayer {
            layer_hash,
            points,
            pop_field,
            total_pop,
        } = layer
        else { panic!("layer") };
        prop_assert_eq!(points, 6);
        // The handle is the whole import answer: the field the parse
        // picked and the people it found (100..600 at the six points).
        prop_assert_eq!(pop_field, "pop");
        prop_assert!((total_pop - 2100.0).abs() < 1e-9);

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
            window: None,
        })
        .expect("coverage");
        let cap_analysis::Response::TypeCoverage { coverage } = coverage else { panic!("cov") };
        prop_assert!(!coverage.is_empty());
        for entry in &coverage {
            prop_assert!(entry.served_pct >= 0.0 && entry.served_pct <= 100.0);
        }
    }

    /// QAF-19: unknown IDs answer empty, unknown HASHES error — one
    /// convention across the module surface. `service_type_detail`
    /// used to error on an unknown type while its sibling lookups
    /// answered empty; the law pins the convention for any id.
    #[test]
    fn unknown_ids_answer_empty_and_unknown_hashes_error(
        trips in proptest::collection::vec(gen_trip(), 1..=14),
        id in "[a-zA-Z][a-zA-Z0-9_]{0,7}",
    ) {
        let net = network(&trips);
        let cap_analysis::Response::LoadNetwork { network_hash } =
            cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net })
                .expect("load")
        else { panic!("load") };
        // The generator's service keys are "0" and "1", so any
        // alphabetic id is unknown to the document.
        let detail = cap_analysis::dispatch(cap_analysis::Request::ServiceTypeDetail {
            network_hash: network_hash.clone(),
            service_type: id.clone(),
        })
        .expect("an unknown id answers, never errors");
        let cap_analysis::Response::ServiceTypeDetail { detail } = detail else { panic!("detail") };
        prop_assert!(detail.is_none(), "unknown type {} answered a detail", id);

        let comparison = cap_analysis::dispatch(cap_analysis::Request::ServiceTypeCompare {
            base_hash: network_hash.clone(),
            network_hash: network_hash.clone(),
            service_type: id,
        })
        .expect("an unknown id answers, never errors");
        let cap_analysis::Response::ServiceTypeCompare { comparison } = comparison else { panic!("cmp") };
        prop_assert!(comparison.is_none(), "an unknown type compared to something");

        // The hash is an address: unknown means error, not empty.
        prop_assert!(
            cap_analysis::dispatch(cap_analysis::Request::ServiceTypeDetail {
                network_hash: "sha256:missing".into(),
                service_type: "0".into(),
            })
            .is_err(),
            "an unknown hash must error, never answer empty"
        );
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
fn a_network_compares_against_itself_as_unchanged() {
    let net = network(&[GenTrip { route: 0, service: 0, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5 }]);
    let hash = match cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net }).unwrap() {
        cap_analysis::Response::LoadNetwork { network_hash } => network_hash,
        _ => panic!("load_network"),
    };
    let service_type = "0".to_string();
    let same = cap_analysis::Request::ServiceTypeCompare { base_hash: hash.clone(), network_hash: hash.clone(), service_type };
    match cap_analysis::dispatch(same).unwrap() {
        cap_analysis::Response::ServiceTypeCompare { comparison } => {
            let comparison = comparison.expect("the fixture carries service type 0");
            assert!(comparison.routes.is_empty());
            assert_eq!(comparison.unchanged_routes, comparison.base.routes);
        }
        _ => panic!("service_type_compare"),
    }
    let unknown = cap_analysis::Request::ServiceTypeCompare {
        base_hash: "sha256:missing".into(),
        network_hash: hash,
        service_type: "0".into(),
    };
    assert!(cap_analysis::dispatch(unknown).unwrap_err().contains("unknown network"));
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

/// The stop's day as gaps (QAF-25): one op answers what used to be a
/// jq fold over the board — departures, the max gap and the
/// departure it follows, and the per-hour profile. The deep oracle
/// law lives in the library's prop suite; this pins the SEAM: the
/// tagged round-trip, the two-trip fixture's one gap, and the
/// id-lookup honesty (an unknown stop answers empty, never an
/// error).
#[test]
fn stop_gaps_answer_through_the_seam() {
    let net = network(&[
        GenTrip { route: 0, service: 0, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5 },
        GenTrip { route: 0, service: 0, block: None, start_slot: 13, stops: vec![0, 1], leg_min: 5 },
    ]);
    let loaded = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net })
        .expect("load");
    let cap_analysis::Response::LoadNetwork { network_hash } = loaded else { panic!("load") };

    let gaps = cap_analysis::dispatch(cap_analysis::Request::StopGaps {
        network_hash: network_hash.clone(),
        date: "2026-01-05".into(),
        stop_id: "s0".into(),
    })
    .expect("stop_gaps");
    let cap_analysis::Response::StopGaps { gaps } = gaps else { panic!("stop_gaps") };
    assert_eq!(gaps.departures, 2);
    // start_slot 10 and 13 → 6000 s and 7800 s: one 1800 s gap.
    assert_eq!(gaps.max_gap_secs, Some(1800));
    assert_eq!(gaps.max_gap_after_secs, Some(6000));
    assert_eq!(gaps.hourly.len(), 2, "6000 s is hour 1, 7800 s is hour 2");
    assert_eq!(gaps.hourly[0].hour, 1);
    assert_eq!(gaps.hourly[0].departures, 1);
    assert_eq!(gaps.hourly[0].max_gap_secs, Some(1800), "the gap starts in hour 1");
    assert_eq!(gaps.hourly[1].hour, 2);
    assert_eq!(gaps.hourly[1].departures, 1);
    assert_eq!(gaps.hourly[1].max_gap_secs, None, "no gap starts in hour 2");
    assert_eq!(gaps.hourly[0].median_gap_secs, None, "one gap is no median");

    // The last stop is an arrival, not a departure — no gap row.
    let terminal = cap_analysis::dispatch(cap_analysis::Request::StopGaps {
        network_hash: network_hash.clone(),
        date: "2026-01-05".into(),
        stop_id: "s1".into(),
    })
    .expect("stop_gaps");
    let cap_analysis::Response::StopGaps { gaps: terminal } = terminal else { panic!("stop_gaps") };
    assert_eq!(terminal.departures, 0, "final-stop visits never count");
    assert!(terminal.hourly.is_empty());

    // An unknown stop answers empty, never an error.
    let unknown = cap_analysis::dispatch(cap_analysis::Request::StopGaps {
        network_hash,
        date: "2026-01-05".into(),
        stop_id: "nope".into(),
    })
    .expect("stop_gaps");
    let cap_analysis::Response::StopGaps { gaps: unknown } = unknown else { panic!("stop_gaps") };
    assert_eq!(unknown.departures, 0);
    assert!(unknown.hourly.is_empty());

    // The JSON seam answers the same tagged form.
    let net = network(&[GenTrip { route: 0, service: 0, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5 }]);    let loaded: serde_json::Value = serde_json::from_str(&cap_analysis::dispatch_json(
        &serde_json::json!({ "op": "load_network", "network": serde_json::to_value(net).unwrap() }).to_string(),
    ))
    .unwrap();
    let hash = loaded["ok"]["network_hash"].as_str().unwrap();
    let via_json: serde_json::Value = serde_json::from_str(&cap_analysis::dispatch_json(
        &serde_json::json!({ "op": "stop_gaps", "network_hash": hash, "date": "2026-01-05", "stop_id": "s0" }).to_string(),
    ))
    .unwrap();
    assert!(via_json["ok"]["gaps"]["departures"].as_u64().unwrap() >= 1);
    let bad: serde_json::Value = serde_json::from_str(&cap_analysis::dispatch_json(
        &serde_json::json!({ "op": "stop_gaps", "network_hash": hash, "date": "garbage", "stop_id": "s0" }).to_string(),
    ))
    .unwrap();
    assert!(bad["error"].as_str().unwrap().contains("invalid date"));
}

/// The day's hourly profile (QAF-4): trips per hour by service type,
/// bucketed by trip start — only hours with trips are rows, and the
/// day's trip count is conserved across them. The deep oracle law
/// lives in the library's prop suite; this pins the SEAM.
#[test]
fn hourly_trips_answer_through_the_seam() {
    let net = network(&[
        GenTrip { route: 0, service: 0, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5 },
        GenTrip { route: 0, service: 0, block: None, start_slot: 13, stops: vec![0, 1], leg_min: 5 },
        GenTrip { route: 0, service: 1, block: None, start_slot: 13, stops: vec![0, 1], leg_min: 5 },
    ]);
    let loaded = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net })
        .expect("load");
    let cap_analysis::Response::LoadNetwork { network_hash } = loaded else { panic!("load") };

    let hours = cap_analysis::dispatch(cap_analysis::Request::HourlyTrips {
        network_hash: network_hash.clone(),
        date: "2026-01-05".into(), // a Monday: service 0 runs, service 1 does not
    })
    .expect("hourly_trips");
    let cap_analysis::Response::HourlyTrips { hours } = hours else { panic!("hourly_trips") };
    // start slots 10 and 13 → hours 1 and 2, one type, one trip each.
    assert_eq!(hours.len(), 2);
    assert_eq!(hours[0].hour, 1);
    assert_eq!(hours[0].trips, 1);
    assert_eq!(hours[0].service_types[0].service_type, "0");
    assert_eq!(hours[1].hour, 2);
    assert_eq!(hours[1].trips, 1);

    // A day nothing runs answers empty, never an error: a weekend-only
    // document asked on a Monday.
    let weekend_only = network(&[GenTrip { route: 0, service: 1, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5 }]);
    let loaded = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: weekend_only })
        .expect("load");
    let cap_analysis::Response::LoadNetwork { network_hash } = loaded else { panic!("load") };
    let quiet = cap_analysis::dispatch(cap_analysis::Request::HourlyTrips {
        network_hash,
        date: "2026-01-05".into(),
    })
    .expect("hourly_trips");
    let cap_analysis::Response::HourlyTrips { hours: quiet } = quiet else { panic!("hourly_trips") };
    assert!(quiet.is_empty(), "a day nothing runs answers empty");
}

/// The schedule texture (QAF-16): headway uniformity and clockface
/// alignment per pattern trips on the type's representative day.
/// The deep oracle law lives in the library's prop suite; this pins
/// the SEAM: the two-trip fixture's one 1800 s gap, the quarter-hour
/// grid count, and the unknown-type honesty.
#[test]
fn schedule_texture_answers_through_the_seam() {
    // Two trips 30 minutes apart, both on the quarter-hour grid.
    let net = network(&[
        GenTrip { route: 0, service: 0, block: None, start_slot: 15, stops: vec![0, 1], leg_min: 5 }, // 9000 s
        GenTrip { route: 0, service: 0, block: None, start_slot: 18, stops: vec![0, 1], leg_min: 5 }, // 10800 s
    ]);
    let loaded = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net })
        .expect("load");
    let cap_analysis::Response::LoadNetwork { network_hash } = loaded else { panic!("load") };

    let texture = cap_analysis::dispatch(cap_analysis::Request::ScheduleTexture {
        network_hash: network_hash.clone(),
        service_type: "0".into(),
    })
    .expect("schedule_texture");
    let cap_analysis::Response::ScheduleTexture { texture } = texture else { panic!("schedule_texture") };
    let texture = texture.expect("the weekday type has a representative day");
    assert_eq!(texture.trips, 2);
    assert_eq!(texture.on_clockface, 2, "4800 and 6600 are both on the grid");
    assert_eq!(texture.pattern_trips.len(), 1, "one schedule");
    let row = &texture.pattern_trips[0];
    assert_eq!(row.trips, 2);
    assert_eq!(row.min_gap_secs, Some(1800));
    assert_eq!(row.max_gap_secs, Some(1800));
    assert_eq!(row.median_gap_secs, None, "one gap is no median (the QAF-55 floor)");
    assert_eq!(row.on_clockface, 2);

    // An unknown type answers null, never an error (QAF-19).
    let unknown = cap_analysis::dispatch(cap_analysis::Request::ScheduleTexture {
        network_hash,
        service_type: "nope".into(),
    })
    .expect("schedule_texture");
    let cap_analysis::Response::ScheduleTexture { texture: unknown } = unknown else { panic!("schedule_texture") };
    assert!(unknown.is_none());
}

/// The corridor read (QAF-22): which routes share stops, and where
/// their service stacks. The deep oracle law lives in the library's
/// prop suite; this pins the SEAM: the two-route fixture answers one
/// pair with its shared stop, and a day nothing runs answers empty.
#[test]
fn route_overlap_answers_through_the_seam() {
    // Routes 0 and 1 (both on weekday service 0), both calling at
    // stop 0 — one shared stop, no shared segments.
    let net = network(&[
        GenTrip { route: 0, service: 0, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5 },
        GenTrip { route: 1, service: 0, block: None, start_slot: 12, stops: vec![0, 2], leg_min: 5 },
    ]);
    let loaded = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net })
        .expect("load");
    let cap_analysis::Response::LoadNetwork { network_hash } = loaded else { panic!("load") };

    let overlaps = cap_analysis::dispatch(cap_analysis::Request::RouteOverlap {
        network_hash: network_hash.clone(),
        date: "2026-01-05".into(),
    })
    .expect("route_overlap");
    let cap_analysis::Response::RouteOverlap { overlaps } = overlaps else { panic!("route_overlap") };
    assert_eq!(overlaps.len(), 1, "routes 0 and 1 share stop 0");
    let pair = &overlaps[0];
    assert_eq!(pair.a, "0");
    assert_eq!(pair.b, "1");
    assert_eq!(pair.shared_stops, 1);
    assert_eq!(pair.shared_segments, 0, "no consecutive pair runs in both");
    assert_eq!(pair.stack_stop.as_deref(), Some("s0"));
    assert_eq!(pair.stack_departures, 2, "one departure each at the shared stop");

    // A day nothing runs answers empty, never an error.
    let quiet = cap_analysis::dispatch(cap_analysis::Request::RouteOverlap {
        network_hash,
        date: "2026-01-10".into(), // a Saturday: service 0 does not run
    })
    .expect("route_overlap");
    let cap_analysis::Response::RouteOverlap { overlaps: quiet } = quiet else { panic!("route_overlap") };
    assert!(quiet.is_empty());
}

/// The hub's timed-transfer view (QAF-27): arrivals paired with the
/// next routes' departures inside the 60–900 s window — the station
/// board's next question. The deep oracle law lives in the library's
/// prop suite; this pins the SEAM: the fixture's one connection, its
/// window, and the id-lookup honesty.
#[test]
fn timed_transfers_answer_through_the_seam() {
    // Route 0 arrives at s0 (06:00); route 1 departs s0 ten minutes
    // later (06:10) — one usable connection, wait 600 s.
    let net = network(&[
        GenTrip { route: 0, service: 0, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5 },
        GenTrip { route: 1, service: 0, block: None, start_slot: 11, stops: vec![0, 2], leg_min: 5 },
    ]);
    let loaded = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net })
        .expect("load");
    let cap_analysis::Response::LoadNetwork { network_hash } = loaded else { panic!("load") };

    let transfers = cap_analysis::dispatch(cap_analysis::Request::TimedTransfers {
        network_hash: network_hash.clone(),
        date: "2026-01-05".into(),
        stop_id: "s0".into(),
        window: None,
    })
    .expect("timed_transfers");
    let cap_analysis::Response::TimedTransfers { transfers } = transfers else { panic!("timed_transfers") };
    assert_eq!(transfers.len(), 1, "route 0's arrival meets route 1's departure");
    let t = &transfers[0];
    assert_eq!((t.from_route.as_str(), t.to_route.as_str()), ("0", "1"));
    assert_eq!(t.arrival_secs, 6000);
    assert_eq!(t.departure_secs, 6600);
    assert_eq!(t.wait_secs, 600);

    // The caller's window decides: narrowed below the 600 s wait,
    // the connection is gone.
    let narrow = cap_analysis::dispatch(cap_analysis::Request::TimedTransfers {
        network_hash: network_hash.clone(),
        date: "2026-01-05".into(),
        stop_id: "s0".into(),
        window: Some(transit_analysis::TransferWindow { min_secs: 60, max_secs: 300 }),
    })
    .expect("timed_transfers");
    let cap_analysis::Response::TimedTransfers { transfers: narrow } = narrow else { panic!("timed_transfers") };
    assert!(narrow.is_empty(), "a 600 s wait does not fit a 300 s window");

    // An unknown stop answers empty, never an error.
    let unknown = cap_analysis::dispatch(cap_analysis::Request::TimedTransfers {
        network_hash,
        date: "2026-01-05".into(),
        stop_id: "nope".into(),
        window: None,
    })
    .expect("timed_transfers");
    let cap_analysis::Response::TimedTransfers { transfers: unknown } = unknown else { panic!("timed_transfers") };
    assert!(unknown.is_empty());
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

#[test]
fn the_kernel_bulk_op_registers_the_same_identity_as_load_network() {
    // The bare document on the kernel's bulk op — parsed straight into
    // the model, no request envelope — answers exactly what the JSON
    // op answers; garbage bytes are an error outcome, never a trap.
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_analysis::dispatch_json(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_analysis::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_analysis::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}

/// The unserved WHERE of coverage (QAF-23): the share's complement,
/// named — every point beyond the radius, with the nearest served
/// stop and how far out it sits. The deep oracle law (and the
/// conservation cross-check with `coverage`) lives in the
/// library's prop suite; this pins the SEAM.
#[test]
fn unserved_points_answer_the_where() {
    // One weekday trip s0→s1; a layer with one point AT s0 and one
    // far away — only the far point is unserved, and the answer
    // names its nearest served stop.
    let net = network(&[GenTrip { route: 0, service: 0, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5 }]);
    let loaded = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net })
        .expect("load");
    let cap_analysis::Response::LoadNetwork { network_hash } = loaded else { panic!("load") };

    let layer = cap_analysis::dispatch(cap_analysis::Request::LoadPopulationLayer {
        bytes: br#"{"type":"FeatureCollection","features":[
            {"type":"Feature","properties":{"pop":100},"geometry":{"type":"Point","coordinates":[-73.5,45.5]}},
            {"type":"Feature","properties":{"pop":50},"geometry":{"type":"Point","coordinates":[-74.0,46.0]}}
        ]}"#
            .to_vec(),
        pop_field: Some("pop".into()),
    })
    .expect("layer");
    let cap_analysis::Response::LoadPopulationLayer { layer_hash, .. } = layer else { panic!("layer") };

    let points = cap_analysis::dispatch(cap_analysis::Request::UnservedPoints {
        network_hash: network_hash.clone(),
        layer_hash: layer_hash.clone(),
        radius_m: 500.0,
        date: "2026-01-05".into(),
    })
    .expect("unserved_points");
    let cap_analysis::Response::UnservedPoints { points } = points else { panic!("unserved_points") };
    assert_eq!(points.len(), 1, "the point at s0 is served; the far one is not");
    assert_eq!(points[0].nearest_stop.as_deref(), Some("s1"), "s1 is marginally nearer than s0");
    assert!((points[0].lat - 46.0).abs() < 1e-9 && (points[0].lon - -74.0).abs() < 1e-9);
    assert!(points[0].nearest_m > 60_000.0, "the far point is ~70 km out: {}", points[0].nearest_m);

    // A day nothing runs: every point is unserved, and no stop is
    // nearest (there is no service to be near).
    let saturday = cap_analysis::dispatch(cap_analysis::Request::UnservedPoints {
        network_hash,
        layer_hash,
        radius_m: 500.0,
        date: "2026-01-10".into(),
    })
    .expect("unserved_points");
    let cap_analysis::Response::UnservedPoints { points: saturday } = saturday else { panic!("unserved_points") };
    assert_eq!(saturday.len(), 2, "nothing runs: every point is unserved");
    assert!(saturday.iter().all(|p| p.nearest_stop.is_none()));
}

/// The service groups (QAF-65): the families behind the ids. A
/// rider-along type — the SAME weekly pattern, a narrower calendar
/// — folds into the base's row and never heads one of its own; a
/// DIFFERENT weekly pattern (Mon+Wed) is its own family, however
/// few days it runs. The deep oracle law lives in the library's
/// prop suite; this pins the SEAM.
#[test]
fn service_groups_answer_the_families() {
    // Route 0 runs under the weekday service (every weekday, all
    // 2026) and a SAME-PATTERN overlay (Mon–Fri, Mar–Jun) — one
    // weekday family, the overlay folded in — plus a Mon+Wed
    // service, which is NOT the weekday family.
    let net = network(&[
        GenTrip { route: 0, service: 0, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5 },
        GenTrip { route: 0, service: 3, block: None, start_slot: 11, stops: vec![0, 1], leg_min: 5 },
        GenTrip { route: 0, service: 2, block: None, start_slot: 12, stops: vec![0, 1], leg_min: 5 },
    ]);
    let loaded = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: net })
        .expect("load");
    let cap_analysis::Response::LoadNetwork { network_hash } = loaded else { panic!("load") };

    let groups = cap_analysis::dispatch(cap_analysis::Request::ServiceGroups {
        network_hash: network_hash.clone(),
    })
    .expect("service_groups");
    let cap_analysis::Response::ServiceGroups { groups } = groups else { panic!("service_groups") };
    assert_eq!(groups.len(), 2, "the weekday family and the Mon+Wed family");
    let weekday = groups.iter().find(|g| g.service_type == "0").expect("weekday row");
    assert_eq!(weekday.members, vec!["0".to_string(), "3".to_string()], "the same-pattern rider is named");
    assert_eq!(weekday.trips, 2, "the family's schedules are pooled in");
    assert_eq!(weekday.active_routes, 1);
    let tue = groups.iter().find(|g| g.service_type == "2").expect("Mon+Wed row");
    assert_eq!(tue.members, vec!["2".to_string()], "a different weekly pattern is its own family");

    // A weekend-only document: the weekend type is its own base.
    let weekend = network(&[GenTrip { route: 0, service: 1, block: None, start_slot: 10, stops: vec![0, 1], leg_min: 5 }]);
    let loaded = cap_analysis::dispatch(cap_analysis::Request::LoadNetwork { network: weekend })
        .expect("load");
    let cap_analysis::Response::LoadNetwork { network_hash } = loaded else { panic!("load") };
    let groups = cap_analysis::dispatch(cap_analysis::Request::ServiceGroups {
        network_hash,
    })
    .expect("service_groups");
    let cap_analysis::Response::ServiceGroups { groups } = groups else { panic!("service_groups") };
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].service_type, "1");
    assert_eq!(groups[0].members, vec!["1".to_string()]);
}
