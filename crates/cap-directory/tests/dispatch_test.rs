//! Native tests of the directory ops over the network that cap-network's import
//! produces from the e2e zip (cap-directory/OPS.md).

use cap_directory::{dispatch, dispatch_bytes, dispatch_json, Request, Response};
use proptest::prelude::*;
use sha2::Digest;
use transit_network::Network;

const FIXTURE: &[u8] = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");

fn derived_network() -> Network {
    let feed = transit_codec::Feed::from_zip_bytes(FIXTURE).expect("fixture parses");
    transit_codec::import(&feed).network
}

fn load_network() -> String {
    cap_abi::kernel::kind::<transit_network::Network>()
        .register(derived_network())
        .unwrap()
        .0
}

fn stops(limit: Option<u32>) -> Vec<cap_directory::DirectoryStop> {
    match dispatch(Request::Stops {
        network_hash: load_network(),
        query: None,
        limit,
    })
    .unwrap()
    {
        Response::Stops { stops } => stops,
        _ => panic!("stops"),
    }
}

#[test]
fn the_network_identity_is_the_modules_canonical_serialization() {
    let hash = load_network();
    let canonical = cap_abi::canonical_bytes(&derived_network()).unwrap();
    let expect = format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()));
    assert_eq!(hash, expect);
    assert_eq!(load_network(), hash);
}

#[test]
fn the_stop_list_is_name_sorted_and_carries_the_walking_group() {
    let all = stops(None);
    assert_eq!(all.len(), 6, "A, B, C, the station pin, two bays");

    let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "{names:?}");

    let by_id = |id: &str| {
        all.iter()
            .find(|s| s.stop_id == id)
            .unwrap_or_else(|| panic!("{id}"))
    };
    assert_eq!(by_id("SA").parent.as_deref(), Some("STN"));
    assert_eq!(by_id("SB").parent.as_deref(), Some("STN"));
    assert_eq!(by_id("STN").parent, None);
    assert_eq!(by_id("A").name, "Main & 1st");

    let two = stops(Some(2));
    assert_eq!(two.len(), 2);
    assert_eq!(two[0].stop_id, all[0].stop_id);
}

#[test]
fn a_query_narrows_the_list_like_a_pickers_typing() {
    let network_hash = load_network();
    let search = |query: &str| -> Vec<String> {
        match dispatch(Request::Stops {
            network_hash: network_hash.clone(),
            query: Some(query.into()),
            limit: None,
        })
        .unwrap()
        {
            Response::Stops { stops } => stops.into_iter().map(|s| s.stop_id).collect(),
            _ => panic!("stops"),
        }
    };
    assert_eq!(
        search("MAIN station"),
        vec!["STN", "SA", "SB"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
    assert_eq!(search("bay b").len(), 2);
    assert_eq!(search("bay sb"), vec!["SB".to_string()]);
    assert_eq!(search("3rd"), vec!["C".to_string()]);
    assert!(
        search("nowhere").is_empty(),
        "no match answers empty, never an error"
    );
    assert_eq!(
        search("main statoin"),
        vec!["STN", "SA", "SB"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
    assert_eq!(search("  ").len(), 6, "an empty query is the whole list");
}

#[test]
fn the_names_join_answers_in_request_order() {
    let result = match dispatch(Request::StopsByIds {
        network_hash: load_network(),
        ids: vec!["B".into(), "ghost".into(), "STN".into(), "A".into()],
    })
    .unwrap()
    {
        Response::StopsByIds { stops } => stops,
        _ => panic!("stops_by_ids"),
    };
    assert_eq!(
        result
            .iter()
            .map(|s| (s.stop_id.as_str(), s.name.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("B", "Main & 2nd"),
            ("STN", "Main Station"),
            ("A", "Main & 1st")
        ],
    );
}

#[test]
fn the_route_list_is_name_sorted_with_model_modes() {
    let routes = match dispatch(Request::Routes {
        network_hash: load_network(),
    })
    .unwrap()
    {
        Response::Routes { routes } => routes,
        _ => panic!("routes"),
    };
    assert!(
        routes.len() >= 3,
        "the fixture carries routes 10, 20, 40: {routes:?}"
    );
    let names: Vec<&str> = routes.iter().map(|r| r.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "{names:?}");
    assert!(
        routes.iter().all(|r| r.mode == transit_network::Mode::Bus),
        "the fixture's routes are buses: {routes:?}"
    );
    assert!(routes.iter().any(|r| r.route_id == "10"));
}

/// Route 10 has three patterns in the fixture. Two run A to B, with a shape and
/// without. One runs A to C to B.
#[test]
fn a_routes_stop_sequences_are_its_patterns() {
    let patterns = match dispatch(Request::RouteStopSequence {
        network_hash: load_network(),
        route_id: "10".into(),
    })
    .unwrap()
    {
        Response::RouteStopSequence { patterns } => patterns,
        _ => panic!("route_stop_sequence"),
    };
    assert_eq!(
        patterns.len(),
        3,
        "the direct runs and the via-C run: {patterns:?}"
    );
    let longest = patterns.iter().max_by_key(|p| p.stops.len()).unwrap();
    let ids: Vec<&str> = longest.stops.iter().map(|s| s.stop_id.as_str()).collect();
    assert!(
        ids.contains(&"A") && ids.contains(&"B") && ids.contains(&"C"),
        "{ids:?}"
    );
    let pos = |id: &str| ids.iter().position(|s| *s == id).unwrap();
    assert!(
        pos("A") < pos("C") && pos("C") < pos("B"),
        "C is interior on the longest pattern"
    );
    assert!(longest.stops.iter().all(|s| !s.name.is_empty()));

    let ghost = match dispatch(Request::RouteStopSequence {
        network_hash: load_network(),
        route_id: "missing".into(),
    })
    .unwrap()
    {
        Response::RouteStopSequence { patterns } => patterns,
        _ => panic!("route_stop_sequence"),
    };
    assert!(ghost.is_empty());
}

#[test]
fn a_route_row_carries_its_long_name() {
    let routes = match dispatch(Request::Routes {
        network_hash: load_network(),
    })
    .unwrap()
    {
        Response::Routes { routes } => routes,
        _ => panic!("routes"),
    };
    let ten = routes.iter().find(|r| r.route_id == "10").unwrap();
    assert_eq!(
        (ten.name.as_str(), ten.long_name.as_deref()),
        ("10", Some("Downtown via Main"))
    );
}

fn near(
    lat: f64,
    lon: f64,
    limit: Option<u32>,
    radius_m: Option<f64>,
) -> Result<Vec<transit_network::listing::NearbyStop>, cap_abi::Refusal> {
    dispatch(Request::StopsNear {
        network_hash: load_network(),
        lat,
        lon,
        limit,
        radius_m,
        walks: None,
    })
    .map(|r| match r {
        Response::StopsNear { stops } => stops,
        _ => panic!("stops_near"),
    })
}

#[test]
fn stops_near_a_point_are_places_nearest_first() {
    let rows = near(45.5150, -73.5155, None, None).unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.stop_id.as_str()).collect();
    assert_eq!(ids[0], "STN", "the bay answers as its station: {ids:?}");
    assert!(rows[0].distance_m < 1e-6);
    assert!(
        !ids.contains(&"SA") && !ids.contains(&"SB"),
        "bays fold into the station: {ids:?}"
    );
    assert_eq!(
        ids.len(),
        4,
        "A, B, C and the station — every place, under the default limit"
    );
    assert!(rows.windows(2).all(|w| w[0].distance_m <= w[1].distance_m));

    assert_eq!(near(45.5150, -73.5155, Some(1), None).unwrap().len(), 1);
    assert!(near(45.5150, -73.5155, None, Some(500.0))
        .unwrap()
        .iter()
        .all(|r| r.distance_m <= 500.0));
    assert!(near(0.0, 0.0, None, Some(1000.0)).unwrap().is_empty());

    assert!(near(91.0, 0.0, None, None)
        .unwrap_err()
        .message
        .contains("lat"));
    assert!(near(0.0, 181.0, None, None)
        .unwrap_err()
        .message
        .contains("lon"));
    assert!(near(0.0, 0.0, None, Some(-1.0))
        .unwrap_err()
        .message
        .contains("radius"));
    let bad =
        dispatch_json(r#"{"op":"stops_near","network_hash":"sha256:x","lat":"here","lon":0}"#);
    assert!(bad.contains("invalid request"), "{bad}");
}

/// A and B are nearer as the crow flies, but the station's walk is shortest.
#[test]
fn stops_near_with_walks_ranks_by_walk() {
    let request = serde_json::json!({
        "op": "stops_near", "network_hash": load_network(), "lat": 45.5075, "lon": -73.5075,
        "walks": [
            { "stop_id": "SB", "walk_secs": 400, "path": [] },
            { "stop_id": "SA", "walk_secs": 300, "path": [] },
        ],
    });
    let answer: serde_json::Value =
        serde_json::from_str(&dispatch_json(&request.to_string())).unwrap();
    let rows = answer["ok"]["stops"].as_array().unwrap();
    let ids: Vec<&str> = rows
        .iter()
        .map(|r| r["stop_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids[0], "STN", "{ids:?}");
    assert_eq!(
        rows[0]["walk_secs"], 300,
        "the station walks as far as its nearest bay"
    );
    assert!(rows[1..].iter().all(|r| r["walk_secs"].is_null()));
    assert!(rows[1..]
        .windows(2)
        .all(|w| w[0]["distance_m"].as_f64() <= w[1]["distance_m"].as_f64()));
}

#[test]
fn unknown_hashes_and_garbage_are_errors_not_panics() {
    let err = dispatch(Request::Stops {
        network_hash: "sha256:missing".into(),
        query: None,
        limit: None,
    })
    .unwrap_err();
    assert!(err.message.contains("unknown network"), "{err}");

    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_json(r#"{"op":"stops","network_hash":"sha256:x","limit":"lots"}"#);
    assert!(
        bad.contains("invalid request"),
        "a non-numeric limit is a request error: {bad}"
    );
    let bad = dispatch_bytes(0, b"whatever");
    assert!(
        bad.contains("unknown bulk op"),
        "the module takes JSON documents: {bad}"
    );
}

#[test]
fn focus_frames_an_entity_on_the_map() {
    let hash = load_network();
    let focus = |kind: transit_network::EntityKind, id: &str| match dispatch(Request::Focus {
        network_hash: hash.clone(),
        kind,
        id: id.into(),
    })
    .unwrap()
    {
        Response::Focus { focus } => focus,
        _ => panic!("focus"),
    };
    let route = focus(transit_network::EntityKind::Route, "10").expect("route 10 exists");
    assert!(!route.lines.is_empty() && !route.points.is_empty());
    assert_eq!(
        focus(transit_network::EntityKind::Stop, "A")
            .unwrap()
            .points
            .len(),
        1
    );
    assert!(focus(transit_network::EntityKind::Route, "nope").is_none());
}

#[test]
fn the_planners_lists_are_directory_answers() {
    let hash = load_network();
    let rows = |kind, query: Option<&str>| match dispatch(Request::Entities {
        network_hash: hash.clone(),
        kind,
        query: query.map(str::to_string),
    })
    .unwrap()
    {
        Response::Entities { rows } => rows,
        _ => panic!("entities"),
    };
    let all = rows(transit_network::EntityKind::Stop, None);
    let first = &all[0];
    let narrowed = rows(
        transit_network::EntityKind::Stop,
        Some(&first.name.to_uppercase()),
    );
    assert!(
        narrowed.iter().any(|r| r.id == first.id),
        "the filter is case-insensitive"
    );
    assert!(narrowed.len() <= all.len());
    assert!(rows(transit_network::EntityKind::PatternTrips, None)
        .iter()
        .all(|r| r.trips.is_some()));

    let detail = match dispatch(Request::RouteDetail {
        network_hash: hash.clone(),
        route_id: "10".into(),
    })
    .unwrap()
    {
        Response::RouteDetail { detail } => detail.expect("route 10 exists"),
        _ => panic!("route_detail"),
    };
    assert!(!detail.patterns.is_empty() && !detail.stops.is_empty());
    assert!(detail
        .pattern_trips_by_type
        .iter()
        .flat_map(|g| &g.pattern_trips)
        .all(|s| s.trips.unwrap_or(0) > 0));
    let direction = detail.directions[0].direction.clone();
    let members = vec![detail.pattern_trips_by_type[0].service_type.clone()];
    let request = Request::Timetable {
        network_hash: hash.clone(),
        route_id: "10".into(),
        direction: direction.clone(),
        members: members.clone(),
        compare_hash: None,
    };
    let table = match dispatch(request).unwrap() {
        Response::Timetable { timetable } => timetable.expect("the direction and type exist"),
        _ => panic!("timetable"),
    };
    assert!(!table.trips.is_empty());
    assert!(
        table
            .trips
            .windows(2)
            .all(|w| w[0].start_secs <= w[1].start_secs),
        "rows by start"
    );
    assert!(table
        .trips
        .iter()
        .all(|t| t.times.len() == table.stops.len()));
    let against = Request::Timetable {
        network_hash: hash.clone(),
        route_id: "10".into(),
        direction,
        members,
        compare_hash: Some(hash.clone()),
    };
    match dispatch(against).unwrap() {
        Response::Timetable { timetable: Some(t) } => {
            assert!(t.trips.iter().all(|x| !x.added) && t.removed.is_empty())
        }
        _ => panic!("timetable"),
    }
    let unknown = Request::Timetable {
        network_hash: hash.clone(),
        route_id: "10".into(),
        direction: Some("ghost".into()),
        members: vec![],
        compare_hash: None,
    };
    match dispatch(unknown).unwrap() {
        Response::Timetable { timetable } => assert!(timetable.is_none()),
        _ => panic!("timetable"),
    }
    match dispatch(Request::RouteDetail {
        network_hash: hash.clone(),
        route_id: "nope".into(),
    })
    .unwrap()
    {
        Response::RouteDetail { detail } => assert!(detail.is_none()),
        _ => panic!("route_detail"),
    }
    let stop_id = detail.stops[0].id.clone();
    let card = match dispatch(Request::StopDetail {
        network_hash: hash.clone(),
        stop_id: stop_id.clone(),
        members: None,
    })
    .unwrap()
    {
        Response::StopDetail { detail } => detail.expect("a visited stop exists"),
        _ => panic!("stop_detail"),
    };
    assert!(
        card.routes.iter().any(|r| r.id == "10"),
        "a stop route 10 visits is served by it"
    );
    assert!(
        card.visits.iter().any(|v| v.route == "10" && v.trips > 0),
        "its visits carry the trips each pattern runs"
    );
    match dispatch(Request::StopDetail {
        network_hash: hash.clone(),
        stop_id,
        members: Some(vec![]),
    })
    .unwrap()
    {
        Response::StopDetail { detail } => {
            let under_none = detail.expect("a visited stop exists");
            assert_eq!(under_none.visits.len(), card.visits.len());
            assert!(under_none.visits.iter().all(|v| v.trips == 0));
        }
        _ => panic!("stop_detail"),
    }
    match dispatch(Request::StopDetail {
        network_hash: hash,
        stop_id: "nope".into(),
        members: None,
    })
    .unwrap()
    {
        Response::StopDetail { detail } => assert!(detail.is_none()),
        _ => panic!("stop_detail"),
    }
}

#[test]
fn the_kernel_bulk_op_registers_the_same_identity_as_load_network() {
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value = serde_json::from_str(&cap_directory::invoke(&format!(
        r#"{{"op":"load_network","network":{doc}}}"#
    )))
    .unwrap();
    let via_bytes: serde_json::Value = serde_json::from_str(&cap_directory::invoke_bytes(
        <transit_network::Network as cap_abi::kernel::Load>::CODE,
        doc.as_bytes(),
    ))
    .unwrap();
    assert!(
        via_json["ok"]["network_hash"]
            .as_str()
            .is_some_and(|h| h.starts_with("sha256:")),
        "{via_json}"
    );
    assert_eq!(via_bytes, via_json);
    let bad = cap_directory::invoke_bytes(
        <transit_network::Network as cap_abi::kernel::Load>::CODE,
        b"{not a network",
    );
    assert!(bad.contains("invalid document"), "{bad}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn unknown_ids_answer_empty_never_errors(id in "[a-z][a-z0-9_]{0,7}") {
        let hash = load_network();
        match dispatch(Request::RouteDetail { network_hash: hash.clone(), route_id: id.clone() }) {
            Ok(Response::RouteDetail { detail }) => prop_assert!(detail.is_none()),
            other => panic!("route_detail on an unknown id must answer, not {other:?}"),
        }
        match dispatch(Request::StopDetail { network_hash: hash.clone(), stop_id: id.clone(), members: None }) {
            Ok(Response::StopDetail { detail }) => prop_assert!(detail.is_none()),
            other => panic!("stop_detail on an unknown id must answer, not {other:?}"),
        }
        match dispatch(Request::Timetable {
            network_hash: hash.clone(),
            route_id: id.clone(),
            direction: None,
            members: vec!["WK".into()],
            compare_hash: None,
        }) {
            Ok(Response::Timetable { timetable }) => prop_assert!(timetable.is_none()),
            other => panic!("timetable on an unknown id must answer, not {other:?}"),
        }
        match dispatch(Request::Timetable {
            network_hash: hash,
            route_id: "10".into(),
            direction: None,
            members: vec![id],
            compare_hash: None,
        }) {
            Ok(Response::Timetable { timetable }) => prop_assert!(timetable.is_none()),
            other => panic!("timetable on an unknown service type must answer, not {other:?}"),
        }
        prop_assert!(
            dispatch(Request::RouteDetail {
                network_hash: "sha256:missing".into(),
                route_id: "10".into(),
            })
            .is_err(),
            "an unknown hash must error, never answer empty"
        );
    }
}

#[test]
fn about_names_the_network_as_the_document_does() {
    let network = derived_network();
    match dispatch(Request::About {
        network_hash: load_network(),
    })
    .unwrap()
    {
        Response::About { name, plan } => {
            assert_eq!(name, network.name);
            assert_eq!(plan, network.plan);
        }
        _ => panic!("about"),
    }
    let mut named = derived_network();
    named.name = "C-TRAN".into();
    named.plan.name = "Fall 2026 pick".into();
    let hash = cap_abi::kernel::kind::<transit_network::Network>()
        .register(named)
        .unwrap()
        .0;
    match dispatch(Request::About { network_hash: hash }).unwrap() {
        Response::About { name, plan } => {
            assert_eq!(name, "C-TRAN");
            assert_eq!(plan.name, "Fall 2026 pick");
        }
        _ => panic!("about"),
    }
}

/// The property is transit-network's `route_lines_draw_what_focus_frames`.
#[test]
fn route_lines_draw_the_network() {
    let hash = load_network();
    let lines = |members: Option<Vec<String>>| match dispatch(Request::RouteLines {
        network_hash: hash.clone(),
        members,
    })
    .unwrap()
    {
        Response::RouteLines { routes } => routes,
        _ => panic!("route_lines"),
    };
    let all = lines(None);
    let ten = all
        .iter()
        .find(|r| r.route_id == "10")
        .expect("route 10 draws");
    let focus =
        transit_network::entity_focus(&derived_network(), transit_network::EntityKind::Route, "10")
            .unwrap();
    assert_eq!(ten.lines.len(), focus.lines.len());
    assert!(lines(Some(vec!["nope".into()])).is_empty());
}

/// The property is transit-network's
/// `service_keys_are_the_authored_calendars_and_what_runs_on_them`.
#[test]
fn service_keys_name_each_calendar_and_what_runs_on_it() {
    let keys = match dispatch(Request::ServiceKeys {
        network_hash: load_network(),
    })
    .unwrap()
    {
        Response::ServiceKeys { keys } => keys,
        _ => panic!("service_keys"),
    };
    let net = derived_network();
    assert_eq!(keys.len(), net.service_types.len());
    let wk = keys.iter().find(|k| k.id == "WK").expect("the weekday key");
    assert_eq!(wk.rule, net.service_types["WK"].rule);
    assert!(wk.routes.iter().any(|r| r.route_id == "10" && r.trips > 0));
}
