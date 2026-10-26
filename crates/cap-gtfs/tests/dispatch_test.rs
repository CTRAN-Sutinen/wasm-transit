//! Native dispatch tests for the gtfs capability — the seam without
//! wasm (the same law the monolith's dispatch_test holds). The
//! content-hash registry gets explicit laws: same bytes, same hash,
//! one entry (idempotence); different bytes, different hashes; and
//! the registry is addressed by hash, so a query for an unknown hash
//! is an error, never a panic.

use std::io::Read;

fn fixture_zip() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../web/e2e/fixtures/sample-gtfs.zip");
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .expect("fixture exists")
        .read_to_end(&mut bytes)
        .expect("fixture readable");
    bytes
}

fn dispatch(request: cap_gtfs::Request) -> cap_gtfs::Response {
    cap_gtfs::dispatch(request).expect("dispatch succeeds")
}

#[test]
fn load_feed_returns_a_content_hash_and_summary() {
    let bytes = fixture_zip();
    let cap_gtfs::Response::LoadFeed { feed_hash, summary } =
        dispatch(cap_gtfs::Request::LoadFeed { bytes: bytes.clone() })
    else {
        panic!("load_feed response");
    };
    assert!(feed_hash.starts_with("sha256:"));
    assert_eq!(summary.stops, 6, "A, B, C, the station pin, two bays");
    assert_eq!(summary.trips, 7);

    // The hash is the content hash of the input bytes.
    assert_eq!(feed_hash, cap_gtfs::content_hash(&bytes));
}

#[test]
fn loading_the_same_bytes_is_idempotent() {
    let bytes = fixture_zip();
    let cap_gtfs::Response::LoadFeed { feed_hash: first, .. } =
        dispatch(cap_gtfs::Request::LoadFeed { bytes: bytes.clone() })
    else {
        panic!("first load")
    };
    let cap_gtfs::Response::LoadFeed { feed_hash: second, .. } =
        dispatch(cap_gtfs::Request::LoadFeed { bytes })
    else {
        panic!("second load")
    };
    assert_eq!(first, second, "same bytes must land on one registry entry");
}

#[test]
fn departures_answer_by_hash_on_the_fixture_date() {
    let bytes = fixture_zip();
    let feed_hash = match dispatch(cap_gtfs::Request::LoadFeed { bytes }) {
        cap_gtfs::Response::LoadFeed { feed_hash, .. } => feed_hash,
        _ => panic!("load"),
    };
    // Friday 2026-10-23: WK plus the HOL addition — the fixture's
    // loaded date.
    let cap_gtfs::Response::Departures { departures } = dispatch(cap_gtfs::Request::Departures {
        feed_hash: feed_hash.clone(),
        stop_id: "A".into(),
        date: "2026-10-23".into(),
        from_secs: 7 * 3600,
        limit: 10,
    })
    else {
        panic!("departures response");
    };
    let times: Vec<u32> = departures.iter().map(|d| d.departure_time.unwrap()).collect();
    assert_eq!(times, vec![8 * 3600, 8 * 3600 + 20 * 60, 8 * 3600 + 40 * 60]);
    let routes: Vec<&str> = departures.iter().map(|d| d.route_name.as_str()).collect();
    assert_eq!(routes, vec!["10", "20", "10"]);
}

#[test]
fn stops_lists_by_hash() {
    let bytes = fixture_zip();
    let feed_hash = match dispatch(cap_gtfs::Request::LoadFeed { bytes }) {
        cap_gtfs::Response::LoadFeed { feed_hash, .. } => feed_hash,
        _ => panic!("load"),
    };
    let cap_gtfs::Response::Stops { stops } = dispatch(cap_gtfs::Request::Stops {
        feed_hash,
        limit: 2,
    })
    else {
        panic!("stops response");
    };
    assert_eq!(stops.len(), 2);
    assert_eq!(stops[0].stop_id, "A");
}

/// The station board: departures pooled over the walking group —
/// the union of every member's per-stop board, merged by time, each
/// row tagged with its concrete bay. The ORACLE is the union itself
/// (per-stop `departures` over the group's members, computed
/// independently here), so the pooling law cannot pass by accident.
#[test]
fn station_departures_pool_the_walking_group() {
    let bytes = fixture_zip();
    let feed_hash = match dispatch(cap_gtfs::Request::LoadFeed { bytes }) {
        cap_gtfs::Response::LoadFeed { feed_hash, .. } => feed_hash,
        _ => panic!("load"),
    };

    let board = |stop_id: &str, limit: u32| {
        match dispatch(cap_gtfs::Request::StationDepartures {
            feed_hash: feed_hash.clone(),
            stop_id: stop_id.into(),
            date: "2026-10-23".into(),
            from_secs: 7 * 3600,
            limit,
        }) {
            cap_gtfs::Response::StationDepartures { departures } => departures,
            _ => panic!("station_departures response"),
        }
    };

    // The parent pin answers the union of its bays (the Fisher's
    // Landing repro at fixture scale: the parent itself has no trips).
    let at_stn = board("STN", 10);
    let mut oracle: Vec<(u32, String, String)> = Vec::new();
    for bay in ["SA", "SB"] {
        let departures = match dispatch(cap_gtfs::Request::Departures {
            feed_hash: feed_hash.clone(),
            stop_id: bay.into(),
            date: "2026-10-23".into(),
            from_secs: 7 * 3600,
            limit: 10,
        }) {
            cap_gtfs::Response::Departures { departures } => departures,
            _ => panic!("departures response"),
        };
        for d in departures {
            oracle.push((d.departure_time.unwrap(), d.stop_id.clone(), d.trip_id.clone()));
        }
    }
    oracle.sort();
    let got: Vec<(u32, String, String)> = at_stn
        .iter()
        .map(|d| (d.departure_time.unwrap(), d.stop_id.clone(), d.trip_id.clone()))
        .collect();
    assert_eq!(got, oracle, "the station board IS the union of the bays");
    assert_eq!(at_stn.len(), 4, "t40/t41 both directions across two bays");
    assert!(at_stn.iter().all(|d| d.stop_id != "STN"), "rows are tagged with the concrete bay");

    // Any member answers the same board: a bay sees its siblings too.
    assert_eq!(
        board("SA", 10).iter().map(|d| d.trip_id.clone()).collect::<Vec<_>>(),
        at_stn.iter().map(|d| d.trip_id.clone()).collect::<Vec<_>>()
    );

    // The limit applies to the POOLED board, after the merge.
    let pooled2 = board("STN", 2);
    assert_eq!(pooled2.len(), 2);
    assert!(pooled2[0].departure_time <= pooled2[1].departure_time);

    // A plain stop's walking group is itself: the station board at A
    // equals the per-stop board at A.
    let pooled_a = board("A", 10);
    let plain_a = match dispatch(cap_gtfs::Request::Departures {
        feed_hash,
        stop_id: "A".into(),
        date: "2026-10-23".into(),
        from_secs: 7 * 3600,
        limit: 10,
    }) {
        cap_gtfs::Response::Departures { departures } => departures,
        _ => panic!("departures response"),
    };
    let shape = |ds: &Vec<transit_core::Departure>| {
        ds.iter().map(|d| (d.departure_time, d.trip_id.clone())).collect::<Vec<_>>()
    };
    assert_eq!(shape(&pooled_a), shape(&plain_a));
}

#[test]
fn unknown_hash_and_bad_input_are_errors_not_panics() {
    let err = cap_gtfs::dispatch(cap_gtfs::Request::Departures {
        feed_hash: "sha256:deadbeef".into(),
        stop_id: "A".into(),
        date: "2026-10-23".into(),
        from_secs: 0,
        limit: 5,
    })
    .expect_err("unknown hash must error");
    assert!(err.contains("unknown feed"));

    let err = cap_gtfs::dispatch(cap_gtfs::Request::LoadFeed { bytes: b"not a zip".to_vec() })
        .expect_err("garbage must error, not panic");
    assert!(err.contains("parse"));
}

#[test]
fn json_seam_round_trips_the_tagged_form() {
    let bytes = fixture_zip();
    let request = serde_json::json!({ "op": "load_feed", "bytes": bytes });
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_gtfs::dispatch_json(&request.to_string())).unwrap();
    assert!(outcome["ok"]["op"] == "load_feed", "tagged response, got {outcome}");
    assert!(outcome["ok"]["feed_hash"].as_str().unwrap().starts_with("sha256:"));

    let bad: serde_json::Value =
        serde_json::from_str(&cap_gtfs::dispatch_json(r#"{"op": "nope"}"#)).unwrap();
    assert!(bad["error"].as_str().unwrap().contains("invalid request"));

    // The bulk-bytes seam loads a feed zip without JSON-encoding it.
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_gtfs::dispatch_bytes(0, &fixture_zip())).unwrap();
    assert!(outcome["ok"]["feed_hash"].as_str().unwrap().starts_with("sha256:"));
    let bad: serde_json::Value =
        serde_json::from_str(&cap_gtfs::dispatch_bytes(9, b"")).unwrap();
    assert!(bad["error"].as_str().unwrap().contains("unknown bulk op"));
}

/// The dispatch reads: what runs today, which blocks (vehicles) are
/// out, one block's trip chain, and the feed's identity. The
/// dispatcher session's top finding — the vehicle/block view is the
/// dispatcher's primary screen, and none of it existed on the bus.
#[test]
fn dispatch_reads_answer_on_the_fixture() {
    let bytes = fixture_zip();
    let feed_hash = match dispatch(cap_gtfs::Request::LoadFeed { bytes }) {
        cap_gtfs::Response::LoadFeed { feed_hash, .. } => feed_hash,
        _ => panic!("load"),
    };

    // Friday 2026-10-23: WK plus the HOL exception — a date carries
    // multiple service keys.
    let services = match dispatch(cap_gtfs::Request::ActiveServices {
        feed_hash: feed_hash.clone(),
        date: "2026-10-23".into(),
    }) {
        cap_gtfs::Response::ActiveServices { services } => services,
        _ => panic!("active_services"),
    };
    assert_eq!(services, vec!["HOL".to_string(), "WK".to_string()], "two keys run on the fixture's loaded date");

    // A no-service day answers empty, never an error (the fixture runs
    // WK Mon–Fri and SUN Sundays — Saturday carries nothing).
    let none = match dispatch(cap_gtfs::Request::ActiveServices {
        feed_hash: feed_hash.clone(),
        date: "2026-10-24".into(),
    }) {
        cap_gtfs::Response::ActiveServices { services } => services,
        _ => panic!("active_services"),
    };
    assert!(none.is_empty(), "Sunday 2026-11-01 has no service");

    // Blocks: all (B1, B2), and the day view (both run on the loaded
    // date; the parked-buses filter keeps what does not run out).
    let all = match dispatch(cap_gtfs::Request::Blocks { feed_hash: feed_hash.clone(), date: None }) {
        cap_gtfs::Response::Blocks { blocks } => blocks,
        _ => panic!("blocks"),
    };
    assert_eq!(all.len(), 2);
    let today = match dispatch(cap_gtfs::Request::Blocks {
        feed_hash: feed_hash.clone(),
        date: Some("2026-10-23".into()),
    }) {
        cap_gtfs::Response::Blocks { blocks } => blocks,
        _ => panic!("blocks"),
    };
    assert_eq!(today.len(), 2, "B1 (WK) and B2 (HOL) both run on the loaded date");
    let saturday = match dispatch(cap_gtfs::Request::Blocks {
        feed_hash: feed_hash.clone(),
        date: Some("2026-10-24".into()),
    }) {
        cap_gtfs::Response::Blocks { blocks } => blocks,
        _ => panic!("blocks"),
    };
    assert!(saturday.is_empty(), "no service, no blocks out");

    // One block's chain: B1 runs t10 then t20, with the layover
    // between them; an unknown block answers empty, never panics.
    let trips = match dispatch(cap_gtfs::Request::BlockTrips {
        feed_hash: feed_hash.clone(),
        block_id: "B1".into(),
        date: "2026-10-23".into(),
    }) {
        cap_gtfs::Response::BlockTrips { trips } => trips,
        _ => panic!("block_trips"),
    };
    assert_eq!(trips.len(), 2);
    assert_eq!(trips[0].trip_id, "t10");
    assert_eq!(trips[1].trip_id, "t20");
    assert_eq!(trips[1].layover_secs, Some(600), "t10 ends 08:10, t20 starts 08:20");
    let ghost = match dispatch(cap_gtfs::Request::BlockTrips {
        feed_hash: feed_hash.clone(),
        block_id: "missing".into(),
        date: "2026-10-23".into(),
    }) {
        cap_gtfs::Response::BlockTrips { trips } => trips,
        _ => panic!("block_trips"),
    };
    assert!(ghost.is_empty());

    // The identity question: which feed is this?
    let info = match dispatch(cap_gtfs::Request::FeedInfo { feed_hash }) {
        cap_gtfs::Response::FeedInfo { info } => info,
        _ => panic!("feed_info"),
    };
    let info = info.expect("the fixture carries feed_info.txt");
    assert_eq!(info.feed_version.as_deref(), Some("2026.10.1"));
    assert_eq!(info.feed_publisher_name.as_deref(), Some("Test Transit"));
}
