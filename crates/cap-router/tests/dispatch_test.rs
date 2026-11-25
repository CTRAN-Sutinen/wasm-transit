//! Pins the op contract of cap-router (`OPS.md`), including that a trip the
//! edit document cancels is never ridden.

use serde_json::{json, Value};
use transit_network::Network;
use transit_sync::{EditRecord, EditStatus};

const FIXTURE: &[u8] = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");
const DATE: &str = "2026-10-23"; // the fixture's loaded Friday

fn derived_network() -> Network {
    let feed = transit_codec::Feed::from_zip_bytes(FIXTURE).expect("fixture parses");
    transit_codec::import(&feed).network
}

fn load_network() -> String {
    cap_abi::kernel::kind::<transit_network::Network>().register(derived_network()).unwrap().0
}

fn record(added_secs: u32) -> EditRecord {
    EditRecord {
        id: "e1".into(),
        diversion: Some(transit_sync::Diversion {
            id: "d1".into(),
            route_id: "10".into(),
            direction_id: None,
            first_kept_stop: "A".into(),
            last_kept_stop: "B".into(),
            skipped_stops: vec!["C".into()],
            detour: None,
            added_secs,
            start_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            end_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            extra: Default::default(),
        }),
        cancel: None,
        status: EditStatus::Accepted,
        rev: 1,
        updated_at: 1,
        extra: Default::default(),
    }
}

fn load_edits(_network_hash: &str, records: Vec<EditRecord>) -> String {
    cap_abi::kernel::kind::<transit_sync::EditDocument>().register(transit_sync::EditDocument::new(records)).unwrap().0
}

#[test]
fn the_document_identity_is_the_canonical_hash_the_bus_agrees_on() {
    let network = derived_network();
    let canonical = cap_abi::canonical_bytes(&network).unwrap();
    use sha2::Digest as _;
    let expected = format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()));
    let request = serde_json::json!({ "op": "load_network", "network": network }).to_string();
    let outcome: serde_json::Value = serde_json::from_str(&cap_router::invoke(&request)).unwrap();
    assert_eq!(outcome["ok"]["network_hash"], expected);
}

#[test]
fn plan_answers_over_the_materialized_day() {
    let hash = load_network();
    let plan = cap_router::dispatch(cap_router::Request::Plan {
        network_hash: hash,
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
        live_hash: None,
        walks_hash: None,
        direct: None,
    })
    .expect("plan dispatch");
    let cap_router::Response::Plan { itinerary: Some(it) } = plan else {
        panic!("A to B is reachable on the fixture date")
    };
    assert_eq!(it.arrival_secs, 8 * 3600 + 10 * 60, "t10: depart 08:00, arrive 08:10");
    match &it.legs[0] {
        transit_router::Leg::Transit { trip, start_secs, .. } => {
            assert_eq!(trip, "t10");
            assert_eq!(*start_secs, 8 * 3600);
        }
        other => panic!("expected a transit leg, got {other:?}"),
    }
}

#[test]
fn plan_is_none_on_an_unreachable_pair_or_day() {
    let hash = load_network();
    // Only the SUN service runs on Sunday (t11 at 09:00), so a departure after
    // the last trip is unreachable.
    let late = cap_router::dispatch(cap_router::Request::Plan {
        network_hash: hash.clone(),
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: "2026-10-25".into(), // Sunday
        depart_secs: 10 * 3600,
        max_legs: 4,
        live_hash: None,
        walks_hash: None,
        direct: None,
    })
    .unwrap();
    let cap_router::Response::Plan { itinerary } = late else { panic!("plan") };
    assert!(itinerary.is_none(), "no service after 10:00 on Sunday");

    let err = cap_router::dispatch(cap_router::Request::Plan {
        network_hash: "sha256:missing".into(),
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 0,
        max_legs: 4,
        live_hash: None,
        walks_hash: None,
        direct: None,
    })
    .expect_err("unknown hash");
    assert!(err.message.contains("unknown network"));
}

#[test]
fn compare_answers_the_diversion_delta() {
    let hash = load_network();
    let edits = load_edits(&hash, vec![record(300)]);

    let cmp = cap_router::dispatch(cap_router::Request::PlanCompare {
        network_hash: hash.clone(),
        edits_hash: Some(edits),
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
        live_hash: None,
        walks_hash: None,
        direct: None,
    })
    .expect("compare dispatch");
    let cap_router::Response::PlanCompare { active, baseline, notice } = cmp else {
        panic!("compare")
    };
    // The diversion adds 300 s to route 10's kept segment. The patched day
    // arrives at 08:15 and the unpatched day at 08:10.
    assert_eq!(active.expect("active plan").arrival_secs, 8 * 3600 + 15 * 60);
    assert_eq!(baseline.expect("baseline world").arrival_secs, 8 * 3600 + 10 * 60);
    let notice = notice.expect("a later arrival carries a notice");
    assert_eq!(notice.kind, transit_router::NoticeKind::Later);
    assert_eq!((notice.delay_secs, notice.minutes_lost), (Some(300), Some(5)));
    assert_eq!(notice.baseline_arrival_secs, Some(8 * 3600 + 10 * 60));
    assert_eq!(notice.routes, vec!["10".to_string()]);
    assert!(notice.text.contains("5 min later"), "{}", notice.text);

    let plain = cap_router::dispatch(cap_router::Request::PlanCompare {
        network_hash: hash,
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
        live_hash: None,
        walks_hash: None,
        direct: None,
    })
    .expect("compare dispatch");
    let cap_router::Response::PlanCompare { active, baseline, .. } = plain else {
        panic!("compare")
    };
    assert!(active.is_some(), "the plan itself still answers");
    assert!(baseline.is_none(), "no edits in force, no baseline");

    let late = cap_router::dispatch(cap_router::Request::PlanCompare {
        network_hash: load_network(),
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 40 * 3600,
        max_legs: 4,
        live_hash: None,
        walks_hash: None,
        direct: None,
    })
    .expect("compare dispatch");
    let cap_router::Response::PlanCompare { active: None, notice: Some(notice), .. } = late else {
        panic!("an unreachable plan carries its notice")
    };
    assert_eq!(notice.kind, transit_router::NoticeKind::NoTrip);
    assert!(notice.text.contains("within 4 legs"), "{}", notice.text);
}

#[test]
fn json_seam_round_trips_the_tagged_form() {
    let hash = load_network();
    let request = serde_json::json!({
        "op": "plan",
        "network_hash": hash,
        "from": "A",
        "to": "B",
        "date": "2026-10-23",
        "depart_secs": 25200,
        "max_legs": 4,
    });
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_router::invoke(&request.to_string())).unwrap();
    assert!(outcome["ok"]["op"] == "plan");
    assert_eq!(outcome["ok"]["itinerary"]["arrival_secs"], 29400);

    let outcome: serde_json::Value =
        serde_json::from_str(&cap_router::invoke_bytes(0, b"{}")).unwrap();
    assert!(outcome["error"]["message"].as_str().unwrap().contains("unknown bulk op"));
}

#[test]
fn an_itinerary_draws_its_ridden_segments() {
    let hash = load_network();
    let cap_router::Response::Plan { itinerary: Some(it) } = cap_router::dispatch(cap_router::Request::Plan {
        network_hash: hash.clone(),
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
        live_hash: None,
        walks_hash: None,
        direct: None,
    })
    .unwrap() else {
        panic!("plan")
    };
    let legs = it.legs.len();
    let cap_router::Response::ItineraryPaths { paths } = cap_router::dispatch(cap_router::Request::ItineraryPaths {
        network_hash: hash,
        edits_hash: None,
        date: DATE.into(),
        itinerary: it,
        from: None,
        to: None,
        direct: None,
    })
    .unwrap() else {
        panic!("itinerary_paths")
    };
    assert_eq!(paths.len(), legs, "one path per leg");
    assert_eq!(paths[0].kind, transit_dispatch::LegKind::Transit);
    assert!(paths[0].path.len() >= 2, "the ridden segment is a line");
}

#[test]
fn the_kernel_bulk_op_registers_the_same_identity_as_load_network() {
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_router::invoke(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_router::invoke_bytes(<transit_network::Network as cap_abi::kernel::Load>::CODE, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_router::invoke_bytes(<transit_network::Network as cap_abi::kernel::Load>::CODE, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}

/// A live document over the fixture, built from JSON so that unread fields
/// keep their defaults. It predicts t10 `delay` s late at A and B.
fn live_t10(delay: i32) -> transit_live::LiveDay {
    let stop = |stop: &str, sched: u32| serde_json::json!({
        "stop": stop, "scheduled_departure": sched, "arrival_delay": delay,
        "departure_delay": delay, "predicted_departure": (sched as i64 + delay as i64) as u32,
    });
    serde_json::from_value(serde_json::json!({
        "kind": "live", "schema_version": 2,
        "window": { "early_secs": 0, "late_secs": 60, "very_late_secs": 300 },
        "trips": [{
            "trip": "t10", "start_secs": 8 * 3600, "route": "10", "route_name": "10",
            "worst_delay_secs": delay, "status": "late",
            "stops": [stop("A", 8 * 3600), stop("B", 8 * 3600 + 600)],
        }],
        "vehicles": [],
        "unmatched_trips": 0,
    }))
    .expect("a valid live document")
}

fn load_live(live: transit_live::LiveDay) -> String {
    cap_abi::kernel::kind::<transit_live::LiveDay>().register(live).expect("load_live").0
}

fn plan_a_b(network_hash: &str, live_hash: Option<String>) -> transit_router::Itinerary {
    match cap_router::dispatch(cap_router::Request::Plan {
        network_hash: network_hash.into(),
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
        live_hash,
        walks_hash: None,
        direct: None,
    })
    .expect("plan")
    {
        cap_router::Response::Plan { itinerary: Some(it) } => it,
        other => panic!("A to B is reachable, got {other:?}"),
    }
}

#[test]
fn the_live_documents_identity_is_its_canonical_hash() {
    let live = live_t10(120);
    use sha2::Digest as _;
    let expected = format!("sha256:{:x}", sha2::Sha256::digest(cap_abi::canonical_bytes(&live).unwrap().as_bytes()));
    assert_eq!(load_live(live), expected);
}

#[test]
fn a_live_plan_rides_the_predicted_times() {
    let hash = load_network();
    let live = load_live(live_t10(120));
    let scheduled = plan_a_b(&hash, None);
    assert_eq!(scheduled.arrival_secs, 8 * 3600 + 600);
    let transit_router::Leg::Transit { board_delay_secs: None, alight_delay_secs: None, .. } = &scheduled.legs[0] else {
        panic!("a schedule-only leg states no delay: {:?}", scheduled.legs[0])
    };
    let predicted = plan_a_b(&hash, Some(live.clone()));
    assert_eq!(predicted.arrival_secs, 8 * 3600 + 720);
    match &predicted.legs[0] {
        transit_router::Leg::Transit { trip, board_secs, board_delay_secs, alight_delay_secs, .. } => {
            assert_eq!(trip, "t10");
            assert_eq!((*board_secs, *board_delay_secs, *alight_delay_secs), (8 * 3600 + 120, Some(120), Some(120)));
        }
        other => panic!("a transit leg, got {other:?}"),
    }
    assert_eq!(plan_a_b(&hash, None).arrival_secs, 8 * 3600 + 600);

    let err = cap_router::dispatch(cap_router::Request::Plan {
        network_hash: hash,
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 7 * 3600,
        max_legs: 4,
        live_hash: Some("sha256:missing".into()),
        walks_hash: None,
        direct: None,
    })
    .expect_err("unknown live hash");
    assert!(err.message.contains("unknown live document"), "{err}");
}

#[test]
fn a_compared_plan_reads_live_in_both_worlds() {
    let hash = load_network();
    let edits = load_edits(&hash, vec![record(300)]);
    let live = load_live(live_t10(120));
    let cap_router::Response::PlanCompare { active, baseline, notice } = cap_router::dispatch(
        cap_router::Request::PlanCompare {
            network_hash: hash,
            edits_hash: Some(edits),
            from: "A".into(),
            to: "B".into(),
            date: DATE.into(),
            depart_secs: 7 * 3600,
            max_legs: 4,
            live_hash: Some(live),
            walks_hash: None,
        direct: None,
        },
    )
    .unwrap() else {
        panic!("compare")
    };
    // The baseline is the unpatched t10, 2 min late, which gives 08:12. The
    // delay of the patched t10 is against the published schedule, and rt
    // already predicts the detour time, so it also gives 08:12. It rides the
    // diverted t10.
    assert_eq!(baseline.unwrap().arrival_secs, 8 * 3600 + 720);
    assert_eq!(active.unwrap().arrival_secs, 8 * 3600 + 720);
    let notice = notice.expect("a diverted ride carries a notice");
    assert_eq!(notice.kind, transit_router::NoticeKind::Diverted);
    assert_eq!(notice.routes, vec!["10".to_string()]);
}

#[test]
fn arrive_by_answers_the_latest_departure() {
    let hash = load_network();
    let arrive_by = |by: Option<u32>| match cap_router::dispatch(cap_router::Request::PlanArriveBy {
        network_hash: hash.clone(),
        edits_hash: None,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        arrive_by_secs: by,
        max_legs: 4,
        live_hash: None,
        walks_hash: None,
        direct: None,
    })
    .expect("plan_arrive_by")
    {
        cap_router::Response::PlanArriveBy { arrive_by_secs, itinerary } => (arrive_by_secs, itinerary),
        other => panic!("plan_arrive_by, got {other:?}"),
    };
    // By 08:35, t20 (08:20 to 08:30) is the latest that makes it.
    let (by, it) = arrive_by(Some(8 * 3600 + 35 * 60));
    assert_eq!(by, 8 * 3600 + 35 * 60);
    let it = it.expect("t20 arrives 08:30");
    assert_eq!((it.depart_secs, it.arrival_secs), (8 * 3600 + 20 * 60, 8 * 3600 + 30 * 60));
    let (by, it) = arrive_by(None);
    assert_eq!(by, 8 * 3600 + 50 * 60, "Friday's last arrival");
    let it = it.expect("the last bus home");
    assert_eq!((it.depart_secs, it.arrival_secs), (8 * 3600 + 40 * 60, 8 * 3600 + 50 * 60));
    assert!(arrive_by(Some(8 * 3600)).1.is_none());
}

#[test]
fn a_leg_names_its_route_and_its_sign() {
    let it = plan_a_b(&load_network(), None);
    match &it.legs[0] {
        transit_router::Leg::Transit { route_name, route_long_name, headsign, .. } => {
            assert_eq!(route_name, "10");
            assert_eq!(route_long_name.as_deref(), Some("Downtown via Main"));
            assert_eq!(headsign, "Downtown", "t10's trip headsign, not the pattern's name");
        }
        other => panic!("a transit leg, got {other:?}"),
    }
}

#[test]
fn the_json_seam_speaks_arrive_by_and_live() {
    let hash = load_network();
    let live = live_t10(60);
    let outcome: serde_json::Value = serde_json::from_str(&cap_router::invoke(
        &serde_json::json!({ "op": "load_live", "live": live }).to_string(),
    ))
    .unwrap();
    let live_hash = outcome["ok"]["live_hash"].as_str().expect("live hash").to_string();
    let request = serde_json::json!({
        "op": "plan_arrive_by", "network_hash": hash, "from": "A", "to": "B",
        "date": DATE, "arrive_by_secs": 30600, "max_legs": 4, "live_hash": live_hash,
    });
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_router::invoke(&request.to_string())).unwrap();
    assert_eq!(outcome["ok"]["op"], "plan_arrive_by", "{outcome}");
    assert_eq!(outcome["ok"]["itinerary"]["depart_secs"], 8 * 3600 + 20 * 60);
    let request = serde_json::json!({
        "op": "plan_arrive_by", "network_hash": hash, "from": "A", "to": "B",
        "date": DATE, "max_legs": 4,
    });
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_router::invoke(&request.to_string())).unwrap();
    assert_eq!(outcome["ok"]["arrive_by_secs"], 8 * 3600 + 50 * 60, "{outcome}");
}

fn walk_table(network_hash: &str, walk_secs: Option<u32>) -> transit_streets::walking::WalkTable {
    let walks = walk_secs
        .map(|secs| {
            vec![
                transit_streets::walking::Walk { from: "SA".into(), to: "SB".into(), walk_secs: secs },
                transit_streets::walking::Walk { from: "SB".into(), to: "SA".into(), walk_secs: secs },
            ]
        })
        .unwrap_or_default();
    transit_streets::walking::WalkTable { kind: Default::default(), schema_version: Default::default(),
        streets_hash: "sha256:streets".into(),
        network_hash: network_hash.into(),
        speed_mps: 1.3,
        max_walk_secs: 600,
        walks,
    }
}

fn plan_bays(network_hash: &str, walks_hash: Option<String>) -> Result<Option<transit_router::Itinerary>, cap_abi::Refusal> {
    // The station's two bays are about 80 m apart and the day's first trip has
    // not left, so a walk between them beats waiting for a bus.
    cap_router::dispatch(cap_router::Request::Plan {
        network_hash: network_hash.into(),
        edits_hash: None,
        from: "SA".into(),
        to: "SB".into(),
        date: DATE.into(),
        depart_secs: 5 * 3600,
        max_legs: 2,
        live_hash: None,
        walks_hash,
        direct: None,
    })
    .map(|r| match r {
        cap_router::Response::Plan { itinerary } => itinerary,
        _ => panic!("plan"),
    })
}

fn load_walks(table: transit_streets::walking::WalkTable) -> String {
    let want = cap_abi::canonical_hash(&table).unwrap();
    let walks_hash = cap_abi::kernel::kind::<transit_streets::walking::WalkTable>().register(table).unwrap().0;
    assert_eq!(walks_hash, want, "the walk table's identity is its canonical hash");
    walks_hash
}

#[test]
fn a_walk_table_is_the_plan_s_transfers() {
    let hash = load_network();
    let walked = |it: &transit_router::Itinerary| matches!(&it.legs[..], [transit_router::Leg::Walk { .. }]);
    let straight = plan_bays(&hash, None).unwrap().expect("straight-line: the bays are a walk apart");
    assert!(walked(&straight), "{:?}", straight.legs);

    let none = load_walks(walk_table(&hash, None));
    if let Some(it) = plan_bays(&hash, Some(none)).unwrap() {
        assert!(!it.legs.iter().any(|l| matches!(l, transit_router::Leg::Walk { .. })), "the table joins the bays by no walk: {:?}", it.legs);
    }

    let five = load_walks(walk_table(&hash, Some(300)));
    let it = plan_bays(&hash, Some(five)).unwrap().expect("the table's walk");
    assert!(walked(&it), "{:?}", it.legs);
    assert_eq!(it.arrival_secs, 5 * 3600 + 300);

    let other = load_walks(walk_table("sha256:another-network", Some(300)));
    let err = plan_bays(&hash, Some(other)).expect_err("another network's table");
    assert!(err.message.contains("walks network"), "{err}");
    let err = plan_bays(&hash, Some("sha256:missing".into())).expect_err("unknown table");
    assert!(err.message.contains("load_walks first"), "{err}");
}

#[test]
fn a_plan_from_a_place_walks_from_it() {
    let hash = load_network();
    let place = |id: &str| json!({ "id": id, "name": "Dropped pin", "walks": [
        { "stop_id": "SA", "walk_secs": 120, "path": [{ "lat": 45.515, "lon": -73.516 }, { "lat": 45.515, "lon": -73.5155 }] },
    ]});
    let plan = |from: Value, extra: Value| {
        let mut req = json!({ "op": "plan", "network_hash": hash, "from": from, "to": "SB", "date": DATE,
                              "depart_secs": 5 * 3600, "max_legs": 2 });
        req.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        serde_json::from_str::<Value>(&cap_router::invoke(&req.to_string())).unwrap()
    };
    let answer = plan(place("here"), json!({}));
    let it = &answer["ok"]["itinerary"];
    assert_eq!(it["from_stop_id"], "here");
    assert_eq!(it["from_stop_name"], "Dropped pin");
    let legs = it["legs"].as_array().unwrap();
    assert_eq!(legs.len(), 1, "one walk: to the bay, then the straight-line walk between the bays: {legs:?}");
    assert_eq!(legs[0]["from_stop_id"], "here");
    assert_eq!(legs[0]["via_stop_ids"], json!(["SA"]));
    assert_eq!(legs[0]["to_stop_id"], "SB");

    let direct = plan(place("here"), json!({ "direct": { "walk_secs": 30 } }));
    assert_eq!(direct["ok"]["itinerary"]["arrival_secs"], 5 * 3600 + 30, "walking straight there is sooner");

    let refused = plan(place("SA"), json!({}));
    assert!(refused["error"]["message"].as_str().unwrap().contains("names a stop"), "{refused}");
}

#[test]
fn a_place_s_walk_draws_along_its_path() {
    let hash = load_network();
    let place = json!({ "id": "here", "name": "Dropped pin", "walks": [
        { "stop_id": "SA", "walk_secs": 120, "path": [
            { "lat": 45.5152, "lon": -73.5165 }, { "lat": 45.515, "lon": -73.5165 }, { "lat": 45.515, "lon": -73.5155 } ] },
    ]});
    let call = |req: Value| -> Value {
        let out: Value = serde_json::from_str(&cap_router::invoke(&req.to_string())).unwrap();
        assert!(out.get("error").is_none(), "{out}");
        out["ok"].clone()
    };
    let it = call(json!({ "op": "plan", "network_hash": hash, "from": place, "to": "SB", "date": DATE,
                          "depart_secs": 5 * 3600, "max_legs": 2 }))["itinerary"].clone();
    let paths = |from: Value, to: Value, direct: Value, it: &Value| {
        call(json!({ "op": "itinerary_paths", "network_hash": hash, "date": DATE, "itinerary": it,
                     "from": from, "to": to, "direct": direct }))["paths"].clone()
    };
    let drawn = paths(place.clone(), json!("SB"), Value::Null, &it);
    let line = drawn[0]["path"].as_array().unwrap();
    assert_eq!(drawn[0]["kind"], "walk");
    assert_eq!(line.len(), 4, "the pin, its street, bay A, then on to bay B: {line:?}");
    assert_eq!(line[0], json!({ "lat": 45.5152, "lon": -73.5165 }));
    assert_eq!(line[3], json!({ "lat": 45.515, "lon": -73.5145 }), "bay B");
    assert_eq!(
        paths(Value::Null, Value::Null, Value::Null, &it)[0]["path"].as_array().unwrap().len(), 2,
        "without the ends, a place is no stop: the walk draws bay A to bay B alone"
    );

    let there = json!({ "id": "there", "name": "Destination", "walks": [] });
    let direct = json!({ "walk_secs": 40, "path": [{ "lat": 45.5152, "lon": -73.5165 }, { "lat": 45.5153, "lon": -73.516 }] });
    let walked = call(json!({ "op": "plan", "network_hash": hash, "from": place, "to": there, "date": DATE,
                              "depart_secs": 5 * 3600, "max_legs": 2, "direct": direct }))["itinerary"].clone();
    assert_eq!(walked["arrival_secs"], 5 * 3600 + 40);
    assert_eq!(paths(place.clone(), there, direct.clone(), &walked)[0]["path"], direct["path"]);
}

#[test]
fn a_cancelled_trip_is_never_ridden() {
    let hash = load_network();
    let plan = |edits_hash: Option<String>| {
        let answer = cap_router::dispatch(cap_router::Request::Plan {
            network_hash: hash.clone(),
            edits_hash,
            from: "A".into(),
            to: "B".into(),
            date: DATE.into(),
            depart_secs: 7 * 3600,
            max_legs: 4,
            live_hash: None,
            walks_hash: None,
            direct: None,
        })
        .unwrap();
        let cap_router::Response::Plan { itinerary } = answer else { panic!("plan") };
        itinerary
    };
    let rides = |it: &Option<transit_router::Itinerary>, want: &str| {
        it.iter().flat_map(|i| &i.legs).any(|l| matches!(l, transit_router::Leg::Transit { trip, .. } if trip == want))
    };
    assert!(rides(&plan(None), "t10"), "without the cancel, t10 is the ride");
    let cancel = EditRecord {
        id: "c1".into(),
        diversion: None,
        cancel: Some(transit_sync::TripCancel {
            id: "cancel-t10".into(),
            trip_id: "t10".into(),
            start_secs: None,
            date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            extra: Default::default(),
        }),
        status: EditStatus::Submitted,
        rev: 1,
        updated_at: 1,
        extra: Default::default(),
    };
    let edits = load_edits(&hash, vec![cancel]);
    let after = plan(Some(edits));
    assert!(!rides(&after, "t10"), "a cancelled trip is never ridden: {after:?}");
    if let Some(it) = &after {
        assert!(it.arrival_secs > 8 * 3600 + 10 * 60, "a later trip, never the cancelled 08:10 arrival");
    }
}
