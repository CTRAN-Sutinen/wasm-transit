//! Native dispatch tests for the directory capability — the seam
//! without wasm, over the REAL fixture-derived network (the same
//! document cap-network's import produces from the e2e zip). The
//! laws:
//!
//! 1. the network document's identity is the module's canonical
//!    serialization (the same identity every network-consuming
//!    module returns);
//! 2. the stop list is name-sorted and carries the walking-group
//!    structure (the parent flag the station board keys off);
//! 3. the names join answers in the request's order, unknown ids
//!    omitted;
//! 4. the route list is name-sorted with the model's modes;
//! 5. a route's stop sequences are its PATTERNS, ordered stops with
//!    resolved names (route_detail groups them by direction, the
//!    timetable runs across one direction's patterns);
//! 6. unknown hashes and unknown routes are honest errors and
//!    empty answers, never panics.

use cap_directory::{dispatch, dispatch_bytes, dispatch_json, Request, Response};
use sha2::Digest;
use transit_network::Network;

const FIXTURE: &[u8] = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");

/// The fixture-derived network document (import is the codec — the
/// boundary; this module composes over the document it produces).
fn derived_network() -> Network {
    let gtfs = transit_core::Gtfs::from_zip_bytes(FIXTURE).expect("fixture parses");
    transit_codec::gtfs_to_network(&gtfs).network
}

fn load_network() -> String {
    match dispatch(Request::LoadNetwork { network: derived_network() }).unwrap() {
        Response::LoadNetwork { network_hash } => network_hash,
        _ => panic!("load_network"),
    }
}

fn stops(limit: Option<u32>) -> Vec<cap_directory::DirectoryStop> {
    match dispatch(Request::Stops { network_hash: load_network(), query: None, limit }).unwrap() {
        Response::Stops { stops } => stops,
        _ => panic!("stops"),
    }
}

#[test]
fn the_network_identity_is_the_modules_canonical_serialization() {
    let hash = load_network();
    // The same formula every network-consuming module uses: the
    // canonical serde bytes hashed. One document, one truth.
    let canonical = serde_json::to_string(&derived_network()).unwrap();
    let expect = format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()));
    assert_eq!(hash, expect);
    // Idempotent: the same document loads to the same hash.
    assert_eq!(load_network(), hash);
}

#[test]
fn the_stop_list_is_name_sorted_and_carries_the_walking_group() {
    let all = stops(None);
    assert_eq!(all.len(), 6, "A, B, C, the station pin, two bays");

    // Name-sorted (the pickers' shape), ties by id — a total order.
    let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "{names:?}");

    // The walking-group structure survives: the bays carry their
    // parent, the pin carries none.
    let by_id = |id: &str| all.iter().find(|s| s.stop_id == id).unwrap_or_else(|| panic!("{id}"));
    assert_eq!(by_id("SA").parent.as_deref(), Some("STN"));
    assert_eq!(by_id("SB").parent.as_deref(), Some("STN"));
    assert_eq!(by_id("STN").parent, None);
    assert_eq!(by_id("A").name, "Main & 1st");

    // The limit truncates AFTER the sort, never silently the list.
    let two = stops(Some(2));
    assert_eq!(two.len(), 2);
    assert_eq!(two[0].stop_id, all[0].stop_id);
}

#[test]
fn a_query_narrows_the_list_like_a_pickers_typing() {
    let network_hash = load_network();
    let search = |query: &str| -> Vec<String> {
        match dispatch(Request::Stops { network_hash: network_hash.clone(), query: Some(query.into()), limit: None })
            .unwrap()
        {
            Response::Stops { stops } => stops.into_iter().map(|s| s.stop_id).collect(),
            _ => panic!("stops"),
        }
    };
    // Every term must match (name or id), case-insensitive: the
    // station and both bays share "main station".
    assert_eq!(search("MAIN station"), vec!["STN", "SA", "SB"].into_iter().map(String::from).collect::<Vec<_>>());
    // Substring terms narrow as a picker's typing does: "bay b"
    // still matches both bays ("b" is in "Bay"); the id settles it.
    assert_eq!(search("bay b").len(), 2);
    assert_eq!(search("bay sb"), vec!["SB".to_string()]);
    assert_eq!(search("3rd"), vec!["C".to_string()]);
    assert!(search("nowhere").is_empty(), "no match answers empty, never an error");
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
    // Unknown ids are omitted — the caller asked for stops — and
    // the answer keeps the request's order.
    assert_eq!(
        result.iter().map(|s| (s.stop_id.as_str(), s.name.as_str())).collect::<Vec<_>>(),
        vec![("B", "Main & 2nd"), ("STN", "Main Station"), ("A", "Main & 1st")],
    );
}

#[test]
fn the_route_list_is_name_sorted_with_model_modes() {
    let routes = match dispatch(Request::Routes { network_hash: load_network() }).unwrap() {
        Response::Routes { routes } => routes,
        _ => panic!("routes"),
    };
    assert!(routes.len() >= 3, "the fixture carries routes 10, 20, 40: {routes:?}");
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

/// A route's stop sequences are its PATTERNS — the model's
/// directions. Route 10 carries two in the fixture (the direct
/// A→B run and the A→C→B run), and the LONGEST is the one with the
/// skippable interior the diversion editor anchors against.
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
    assert_eq!(patterns.len(), 2, "the direct run and the via-C run: {patterns:?}");
    let longest = patterns.iter().max_by_key(|p| p.stops.len()).unwrap();
    let ids: Vec<&str> = longest.stops.iter().map(|s| s.stop_id.as_str()).collect();
    assert!(ids.contains(&"A") && ids.contains(&"B") && ids.contains(&"C"), "{ids:?}");
    // The interior stop sits between the anchors — the editor's
    // skippable middle (the diversion e2e's contract).
    let pos = |id: &str| ids.iter().position(|s| *s == id).unwrap();
    assert!(pos("A") < pos("C") && pos("C") < pos("B"), "C is interior on the longest pattern");
    // Names are resolved: the rows a human reads.
    assert!(longest.stops.iter().all(|s| !s.name.is_empty()));

    // An unknown route answers empty, never an error.
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
fn unknown_hashes_and_garbage_are_errors_not_panics() {
    let err = dispatch(Request::Stops { network_hash: "sha256:missing".into(), query: None, limit: None }).unwrap_err();
    assert!(err.contains("unknown network"), "{err}");

    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_json(r#"{"op":"stops","network_hash":"sha256:x","limit":"lots"}"#);
    assert!(bad.contains("invalid request"), "a non-numeric limit is a request error: {bad}");
    let bad = dispatch_bytes(0, b"whatever");
    assert!(bad.contains("unknown bulk op"), "the module takes JSON documents: {bad}");
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
    assert_eq!(focus(transit_network::EntityKind::Stop, "A").unwrap().points.len(), 1);
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
    let narrowed = rows(transit_network::EntityKind::Stop, Some(&first.name.to_uppercase()));
    assert!(narrowed.iter().any(|r| r.id == first.id), "the filter is case-insensitive");
    assert!(narrowed.len() <= all.len());
    assert!(rows(transit_network::EntityKind::PatternTrips, None).iter().all(|r| r.trips.is_some()));

    let detail = match dispatch(Request::RouteDetail { network_hash: hash.clone(), route_id: "10".into() }).unwrap() {
        Response::RouteDetail { detail } => detail.expect("route 10 exists"),
        _ => panic!("route_detail"),
    };
    assert!(!detail.patterns.is_empty() && !detail.stops.is_empty());
    assert!(detail.pattern_trips_by_type.iter().flat_map(|g| &g.pattern_trips).all(|s| s.trips.unwrap_or(0) > 0));
    // The timetable of its first direction under its first service
    // type: every trip, by start, one time per column.
    let direction = detail.directions[0].direction.clone();
    let service_type = detail.pattern_trips_by_type[0].service_type.clone();
    let request = Request::Timetable {
        network_hash: hash.clone(),
        route_id: "10".into(),
        direction: direction.clone(),
        service_type: service_type.clone(),
        compare_hash: None,
    };
    let table = match dispatch(request).unwrap() {
        Response::Timetable { timetable } => timetable.expect("the direction and type exist"),
        _ => panic!("timetable"),
    };
    assert!(!table.trips.is_empty());
    assert!(table.trips.windows(2).all(|w| w[0].start_secs <= w[1].start_secs), "rows by start");
    assert!(table.trips.iter().all(|t| t.times.len() == table.stops.len()));
    // Against itself: nothing added, nothing removed.
    let against = Request::Timetable { network_hash: hash.clone(), route_id: "10".into(), direction, service_type, compare_hash: Some(hash.clone()) };
    match dispatch(against).unwrap() {
        Response::Timetable { timetable: Some(t) } => assert!(t.trips.iter().all(|x| !x.added) && t.removed.is_empty()),
        _ => panic!("timetable"),
    }
    let unknown = Request::Timetable { network_hash: hash.clone(), route_id: "10".into(), direction: Some("ghost".into()), service_type: String::new(), compare_hash: None };
    match dispatch(unknown).unwrap() {
        Response::Timetable { timetable } => assert!(timetable.is_none()),
        _ => panic!("timetable"),
    }
    match dispatch(Request::RouteDetail { network_hash: hash.clone(), route_id: "nope".into() }).unwrap() {
        Response::RouteDetail { detail } => assert!(detail.is_none()),
        _ => panic!("route_detail"),
    }
    let serving = match dispatch(Request::StopDetail { network_hash: hash.clone(), stop_id: detail.stops[0].id.clone() }).unwrap() {
        Response::StopDetail { detail } => detail.expect("a visited stop exists").routes,
        _ => panic!("stop_detail"),
    };
    assert!(serving.iter().any(|r| r.id == "10"), "a stop route 10 visits is served by it");
    match dispatch(Request::StopDetail { network_hash: hash, stop_id: "nope".into() }).unwrap() {
        Response::StopDetail { detail } => assert!(detail.is_none()),
        _ => panic!("stop_detail"),
    }
}

#[test]
fn the_kernel_bulk_op_registers_the_same_identity_as_load_network() {
    // The bare document on the kernel's bulk op — parsed straight into
    // the model, no request envelope — answers exactly what the JSON
    // op answers; garbage bytes are an error outcome, never a trap.
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_directory::dispatch_json(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_directory::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_directory::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}
