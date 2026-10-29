//! Native dispatch tests for the router capability — the SEAM over
//! the NETWORK document and the EFFECTIVE DAY. The laws (the
//! algorithm carries its own suite in transit-router; these pin the
//! seam):
//!
//! 1. the network document's identity is the module's canonical
//!    serialization — the same identity every day-view capability
//!    returns for the same document;
//! 2. a plan is the materialized day (model trips, model
//!    vocabulary), answered by hash pair + date;
//! 3. the compare facet is the diversion delta: the patched day
//!    against the unpatched one — one materialization apart, and no
//!    baseline when no edit is in force;
//! 4. unknown hashes and garbage are errors, never panics;
//! 5. the compare facet's notice fires on ANY change the diversion
//!    makes to the trip (no trip, a later arrival, a diverted ride),
//!    structured, with its text line;
//! 6. with a live document, plans ride the predicted times (the
//!    graph keyed by the live hash too), and legs say so;
//! 7. arrive-by answers the latest departure that arrives by the
//!    deadline — the service day's end when none is given;
//! 8. a leg names its route (short and long) and the sign at the
//!    boarding visit;
//! 9. with a walk table, a plan's transfers are its walks (the graph
//!    keyed by the walks hash too); a table derived for another
//!    network is refused.

use serde_json::{json, Value};
use transit_network::Network;
use transit_sync::{EditRecord, EditStatus};

const FIXTURE: &[u8] = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");
const DATE: &str = "2026-10-23"; // the fixture's loaded Friday

/// The fixture-derived network document (import is the codec — the
/// boundary; this module composes over the document it produces).
fn derived_network() -> Network {
    let feed = transit_codec::Feed::from_zip_bytes(FIXTURE).expect("fixture parses");
    transit_codec::import(&feed).network
}

fn load_network() -> String {
    match cap_router::dispatch(cap_router::Request::LoadNetwork { network: derived_network() }) {
        Ok(cap_router::Response::LoadNetwork { network_hash }) => network_hash,
        _ => panic!("load_network"),
    }
}

/// The fixture's diversion: route 10 keeps A→B, skips C, +N s.
fn record(added_secs: u32) -> EditRecord {
    EditRecord {
        id: "e1".into(),
        diversion: transit_sync::Diversion {
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
        },
        status: EditStatus::Accepted,
        rev: 1,
        updated_at: 1,
    }
}

fn load_edits(network_hash: &str, records: Vec<EditRecord>) -> String {
    match cap_router::dispatch(cap_router::Request::LoadEdits {
        network_hash: network_hash.into(),
        records,
    })
    .unwrap()
    {
        cap_router::Response::LoadEdits { edits_hash, .. } => edits_hash,
        _ => panic!("load_edits"),
    }
}

#[test]
fn the_document_identity_is_the_canonical_hash_the_bus_agrees_on() {
    // The composition law: documents are addressed by the hash of
    // the module's canonical serialization, so the host can push the
    // same document into every capability that needs it and assert
    // they agree. Verified against the hash computed independently
    // (the same formula every capability and the manifest use).
    let network = derived_network();
    let canonical = serde_json::to_string(&network).unwrap();
    use sha2::Digest as _;
    let expected = format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()));
    match cap_router::dispatch(cap_router::Request::LoadNetwork { network }) {
        Ok(cap_router::Response::LoadNetwork { network_hash }) => assert_eq!(network_hash, expected),
        _ => panic!("router load_network"),
    }
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
    // The leg's identity is the trip instance — (trip, start_secs) —
    // the identity every day view agrees on.
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
    // Sunday: only the SUN service runs (t11 at 09:00). A departure
    // after the last trip is unreachable.
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

    // Unknown hash: an error, not a panic.
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
    assert!(err.contains("unknown network"));
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
    // The diversion adds 300 s to route 10's kept segment: the
    // patched day arrives 08:15; the unpatched one 08:10 — and the
    // rider is told: 5 min later, on route 10's detour (QAF-74).
    assert_eq!(active.expect("active plan").arrival_secs, 8 * 3600 + 15 * 60);
    assert_eq!(baseline.expect("baseline world").arrival_secs, 8 * 3600 + 10 * 60);
    let notice = notice.expect("a later arrival carries a notice");
    assert_eq!(notice.kind, transit_router::NoticeKind::Later);
    assert_eq!((notice.delay_secs, notice.minutes_lost), (Some(300), Some(5)));
    assert_eq!(notice.baseline_arrival_secs, Some(8 * 3600 + 10 * 60));
    assert_eq!(notice.routes, vec!["10".to_string()]);
    assert!(notice.text.contains("5 min later"), "{}", notice.text);

    // No edit document: there is no second world to compare
    // against — the baseline is honestly absent.
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

    // Too late for any trip: the notice says so, legs named.
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
        serde_json::from_str(&cap_router::dispatch_json(&request.to_string())).unwrap();
    assert!(outcome["ok"]["op"] == "plan");
    assert_eq!(outcome["ok"]["itinerary"]["arrival_secs"], 29400);

    // The router takes JSON documents; a bulk-bytes call is an
    // honest error, never a silent ignore.
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_router::dispatch_bytes(0, b"{}")).unwrap();
    assert!(outcome["error"].as_str().unwrap().contains("unknown bulk op"));
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
    // The bare document on the kernel's bulk op — parsed straight into
    // the model, no request envelope — answers exactly what the JSON
    // op answers; garbage bytes are an error outcome, never a trap.
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_router::dispatch_json(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_router::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_router::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}

/// A live document over the fixture (built from JSON, so fields this
/// module does not read keep their defaults): t10 predicted
/// `delay` s late at A and B.
fn live_t10(delay: i32) -> transit_live::LiveDay {
    let stop = |stop: &str, sched: u32| serde_json::json!({
        "stop": stop, "scheduled_departure": sched, "arrival_delay": delay,
        "departure_delay": delay, "predicted_departure": (sched as i64 + delay as i64) as u32,
    });
    serde_json::from_value(serde_json::json!({
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
    match cap_router::dispatch(cap_router::Request::LoadLive { live }).expect("load_live") {
        cap_router::Response::LoadLive { live_hash } => live_hash,
        _ => panic!("load_live"),
    }
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
    let expected = format!("sha256:{:x}", sha2::Sha256::digest(serde_json::to_string(&live).unwrap().as_bytes()));
    assert_eq!(load_live(live), expected);
}

#[test]
fn a_live_plan_rides_the_predicted_times() {
    let hash = load_network();
    let live = load_live(live_t10(120));
    // Schedule: t10 leaves 08:00, arrives 08:10 — no predictions.
    let scheduled = plan_a_b(&hash, None);
    assert_eq!(scheduled.arrival_secs, 8 * 3600 + 600);
    let transit_router::Leg::Transit { board_delay_secs: None, alight_delay_secs: None, .. } = &scheduled.legs[0] else {
        panic!("a schedule-only leg states no delay: {:?}", scheduled.legs[0])
    };
    // Live: t10 predicted 2 min late — leaves 08:02, arrives 08:12,
    // and the leg says both times are predictions.
    let predicted = plan_a_b(&hash, Some(live.clone()));
    assert_eq!(predicted.arrival_secs, 8 * 3600 + 720);
    match &predicted.legs[0] {
        transit_router::Leg::Transit { trip, board_secs, board_delay_secs, alight_delay_secs, .. } => {
            assert_eq!(trip, "t10");
            assert_eq!((*board_secs, *board_delay_secs, *alight_delay_secs), (8 * 3600 + 120, Some(120), Some(120)));
        }
        other => panic!("a transit leg, got {other:?}"),
    }
    // The schedule-only graph is not the live one (keyed apart).
    assert_eq!(plan_a_b(&hash, None).arrival_secs, 8 * 3600 + 600);

    // Unknown live document: an error, never a silent schedule.
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
    assert!(err.contains("unknown live document"), "{err}");
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
    // Baseline: the unpatched t10, 2 min late → 08:12. Active: the
    // patched t10's delay is against the PUBLISHED schedule (its
    // detour time is already in what rt predicts) → 08:12 too, and
    // it rides the diverted t10.
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
    // By 08:35: t20 (08:20 → 08:30) is the latest that makes it.
    let (by, it) = arrive_by(Some(8 * 3600 + 35 * 60));
    assert_eq!(by, 8 * 3600 + 35 * 60);
    let it = it.expect("t20 arrives 08:30");
    assert_eq!((it.depart_secs, it.arrival_secs), (8 * 3600 + 20 * 60, 8 * 3600 + 30 * 60));
    // No deadline: the last bus home — t30 08:40 → 08:50, the day's
    // last arrival (the deadline answered is the day's end).
    let (by, it) = arrive_by(None);
    assert_eq!(by, 8 * 3600 + 50 * 60, "Friday's last arrival");
    let it = it.expect("the last bus home");
    assert_eq!((it.depart_secs, it.arrival_secs), (8 * 3600 + 40 * 60, 8 * 3600 + 50 * 60));
    // Before the first arrival: nothing.
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
    let outcome: serde_json::Value = serde_json::from_str(&cap_router::dispatch_json(
        &serde_json::json!({ "op": "load_live", "live": live }).to_string(),
    ))
    .unwrap();
    let live_hash = outcome["ok"]["live_hash"].as_str().expect("live hash").to_string();
    let request = serde_json::json!({
        "op": "plan_arrive_by", "network_hash": hash, "from": "A", "to": "B",
        "date": DATE, "arrive_by_secs": 30600, "max_legs": 4, "live_hash": live_hash,
    });
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_router::dispatch_json(&request.to_string())).unwrap();
    assert_eq!(outcome["ok"]["op"], "plan_arrive_by", "{outcome}");
    assert_eq!(outcome["ok"]["itinerary"]["depart_secs"], 8 * 3600 + 20 * 60);
    // The deadline is optional on the wire.
    let request = serde_json::json!({
        "op": "plan_arrive_by", "network_hash": hash, "from": "A", "to": "B",
        "date": DATE, "max_legs": 4,
    });
    let outcome: serde_json::Value =
        serde_json::from_str(&cap_router::dispatch_json(&request.to_string())).unwrap();
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
    transit_streets::walking::WalkTable {
        streets_hash: "sha256:streets".into(),
        network_hash: network_hash.into(),
        speed_mps: 1.3,
        max_walk_secs: 600,
        walks,
    }
}

fn plan_bays(network_hash: &str, walks_hash: Option<String>) -> Result<Option<transit_router::Itinerary>, String> {
    // The station's two bays, ~80 m apart, before the day's first
    // trip: a walk, if one joins them, beats waiting for a bus.
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
    match cap_router::dispatch(cap_router::Request::LoadWalks { walks: table }).unwrap() {
        cap_router::Response::LoadWalks { walks_hash } => {
            assert_eq!(walks_hash, want, "the walk table's identity is its canonical hash");
            walks_hash
        }
        _ => panic!("load_walks"),
    }
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
    assert!(err.contains("walks network"), "{err}");
    let err = plan_bays(&hash, Some("sha256:missing".into())).expect_err("unknown table");
    assert!(err.contains("load_walks first"), "{err}");
}

/// A plan's ends over the JSON seam: a bare string is a stop, an
/// object a place with its walks — cap-streets' `access` walks pass
/// as they are (their paths ride along unread). The trip walks from
/// the place; a place named like a stop is refused.
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
        serde_json::from_str::<Value>(&cap_router::dispatch_json(&req.to_string())).unwrap()
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
    assert!(refused["error"].as_str().unwrap().contains("names a stop"), "{refused}");
}

/// A place's walk legs draw along the streets: given the plan's ends,
/// the walk from the place runs its walk's path, then on through the
/// stops; walking straight between two places runs the direct walk's
/// path. Without the ends, a place is no stop: only the stops draw.
#[test]
fn a_place_s_walk_draws_along_its_path() {
    let hash = load_network();
    let place = json!({ "id": "here", "name": "Dropped pin", "walks": [
        { "stop_id": "SA", "walk_secs": 120, "path": [
            { "lat": 45.5152, "lon": -73.5165 }, { "lat": 45.515, "lon": -73.5165 }, { "lat": 45.515, "lon": -73.5155 } ] },
    ]});
    let call = |req: Value| -> Value {
        let out: Value = serde_json::from_str(&cap_router::dispatch_json(&req.to_string())).unwrap();
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
