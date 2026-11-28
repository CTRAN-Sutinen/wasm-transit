//! The fixtures that the worked examples read. The generator, a
//! capability's `examples` feature, or a dev-dependency may name this crate.
//! No module's closure holds it (arch gates,
//! `cap-abi/src/bin/common/gates.rs`).

#![forbid(unsafe_code)]

use sha2::{Digest as _, Sha256};

pub fn fixture_watchlist() -> transit_rider::watchlist::Watchlist {
    let stop = |id: &str, label: &str| transit_rider::watchlist::WatchStop {
        id: id.into(),
        label: label.into(),
        extra: Default::default(),
    };
    transit_rider::watchlist::Watchlist::new(vec![stop("A", "home"), stop("B", "work")])
}

pub struct NetworkExampleContext {
    pub network: transit_network::Network,
    pub derived: serde_json::Value,
    pub boundary: serde_json::Value,
    pub boundary_hash: String,
    pub notes: serde_json::Value,
    pub network_hash: String,
    pub export_feed_hash: String,
    pub export_summary: serde_json::Value,
    pub zip_b64: String,
}

impl NetworkExampleContext {
    pub fn derived_network(&self) -> &transit_network::Network {
        &self.network
    }
}

/// The fixture as the fall 2026 pick (from 2026-10-19), and a published
/// set: that pick, a retired pick on 2026-11-30, and the winter pick on
/// 2027-01-04.
pub fn pick_example(
    net: &NetworkExampleContext,
) -> Option<(transit_network::Network, transit_plans::PublishedPlans)> {
    let mut pick = net.derived_network().clone();
    pick.plan.id = "fall-2026".into();
    pick.plan.name = "Fall 2026".into();
    pick.plan.effective_from = transit_date::Date::from_ymd_opt(2026, 10, 19);
    let entry = |id: &str, name: &str, from: &str, status: &str| serde_json::json!({ "id": id, "name": name, "effective_from": from, "network_hash": "sha256:…", "status": status });
    let published = serde_json::from_value(serde_json::json!({
        "kind": "plans", "schema_version": 1,
        "plans": [entry("fall-2026", "Fall 2026", "2026-10-19", "published"), entry("holiday-2026", "Holiday 2026", "2026-11-30", "retired"), entry("winter-2027", "Winter 2027", "2027-01-04", "published")],
    }))
    .ok()?;
    Some((pick, published))
}

pub fn network_example_context(feed_bytes: &[u8]) -> Option<NetworkExampleContext> {
    let outcome = transit_codec::import(&transit_codec::Feed::from_zip_bytes(feed_bytes).ok()?);
    let network_hash = cap_abi::canonical_hash(&outcome.network).ok()?;
    let derived = serde_json::to_value(&outcome.network).ok()?;
    let boundary = serde_json::to_value(&outcome.boundary).ok()?;
    let boundary_hash = cap_abi::canonical_hash(&outcome.boundary).ok()?;
    let notes = serde_json::to_value(&outcome.notes).ok()?;
    let exported = transit_codec::export(&outcome.network, &outcome.boundary).ok()?;
    let export_summary = serde_json::to_value(transit_codec::FeedSummary {
        agencies: exported.agency.len(),
        routes: exported.routes.len(),
        stops: exported.stops.len(),
        trips: exported.trips.len(),
        stop_times: exported.stop_times.len(),
    })
    .ok()?;
    let zip = exported.to_zip_bytes();
    let export_feed_hash = format!("sha256:{:x}", Sha256::digest(&zip));
    let zip_b64 = transit_codec::base64_encode(&zip);
    Some(NetworkExampleContext {
        network: outcome.network,
        derived,
        boundary,
        boundary_hash,
        notes,
        network_hash,
        export_feed_hash,
        export_summary,
        zip_b64,
    })
}

pub fn fixture_gtfs() -> Option<transit_codec::Feed> {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/e2e/fixtures/sample-gtfs.zip"
    );
    let bytes = std::fs::read(fixture).ok()?;
    transit_codec::Feed::from_zip_bytes(&bytes).ok()
}

/// The e2e street fixture, with the snap and route answers at the Elbow
/// corner.
pub struct StreetsExampleContext {
    pub streets: transit_streets::Streets,
    pub streets_hash: String,
    pub segments: u32,
    pub nodes: u32,
    pub pos_a: transit_streets::StreetPosition,
    pub pos_b: transit_streets::StreetPosition,
    pub points: Vec<transit_streets::StreetPoint>,
    pub length_m: Option<f64>,
    pub streets_along: Vec<transit_streets::StreetUse>,
}

pub fn streets_example_context() -> Option<StreetsExampleContext> {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/e2e/fixtures/streets.geojson"
    ))
    .ok()?;
    let streets = transit_streets::geojson::import(&bytes).ok()?;
    let streets_hash = cap_abi::canonical_hash(&streets).ok()?;
    let net = transit_streets::StreetNetwork::build(&streets);
    let (segments, nodes) = (net.segment_count(), net.node_count());
    let pos_a = net.snap(45.5040, -73.5030)?;
    let pos_b = net.snap(45.5080, -73.5080)?;
    let (points, length_m, streets_along) = match net.route_along(pos_a, pos_b) {
        Some(route) => (route.points, Some(route.length_m), route.streets),
        None => (vec![], None, vec![]),
    };
    Some(StreetsExampleContext {
        streets,
        streets_hash,
        segments,
        nodes,
        pos_a,
        pos_b,
        points,
        length_m,
        streets_along,
    })
}

pub struct RtExampleContext {
    pub network: transit_network::Network,
    pub network_hash: String,
    pub boundary: transit_codec::Boundary,
    pub boundary_hash: String,
    pub overview: transit_rt::RtOverview,
    pub rt_hash: String,
}

pub fn rt_example_context() -> Option<RtExampleContext> {
    let outcome = transit_codec::import(&fixture_gtfs()?);
    let payload = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/e2e/fixtures/sample-rt.pb"
    ))
    .ok()?;
    let overview = transit_rt::decode_overview(&payload).ok()?;
    Some(RtExampleContext {
        network_hash: cap_abi::canonical_hash(&outcome.network).ok()?,
        boundary_hash: cap_abi::canonical_hash(&outcome.boundary).ok()?,
        network: outcome.network,
        boundary: outcome.boundary,
        overview,
        rt_hash: format!("sha256:{:x}", Sha256::digest(&payload)),
    })
}

pub fn live_example() -> Option<(transit_live::LiveDay, String)> {
    let ctx = rt_example_context()?;
    let live = transit_rt::live_day(
        &ctx.network,
        &ctx.boundary,
        Some(&ctx.overview),
        None,
        None,
        transit_live::OnTimeWindow::default(),
    );
    let live_hash = cap_abi::canonical_hash(&live).ok()?;
    Some((live, live_hash))
}

/// A stops document: one stop in service, given
/// amenities, suspended for a while, then moved; one planned, then
/// permitted.
pub fn fixture_stops() -> transit_stops::Stops {
    serde_json::from_value(serde_json::json!({
        "kind": "stops", "schema_version": 1,
        "stops": {
            "6124": [
                {"on": "2004-09-01", "event": "in_service", "lat": 45.6934, "lon": -122.6512, "name": "NE 99th St & Hwy 99", "code": "6124", "wheelchair": true},
                {"on": "2025-05-12", "event": "changed", "amenities": {"shelter": true, "bench": true, "bin": true}},
                {"on": "2027-03-03", "event": "suspended", "until": "2027-07-01", "note": "water main"},
                {"on": "2027-09-01", "event": "moved", "lat": 45.6936, "lon": -122.6509}
            ],
            "7001": [
                {"on": "2027-01-10", "event": "planned", "lat": 45.7, "lon": -122.66, "name": "NE 104th St"},
                {"on": "2027-05-03", "event": "permitted"}
            ]
        }
    }))
    .map(transit_stops::Stops::identified)
    .expect("the fixture stops parse")
}

/// The fixture Friday, 2026-10-23, in the morning, in Unix seconds.
pub const ALERTS_NOW: u64 = 1_792_756_800;

/// The alerts are route 10's detour at stop C (English and Spanish, explaining
/// the fixture's diversion), route 40's reduced service in one direction for a
/// day, and trip t10's 08:00 instance, which the live feed cancels.
pub fn fixture_alerts() -> transit_alerts::ServiceAlerts {
    serde_json::from_value(serde_json::json!({
        "kind": "service_alerts", "schema_version": 2,
        "alerts": {
            "detour-10": {
                "status": "published", "cause": "construction", "effect": "detour", "severity": "warning",
                "header": [{"text": "Route 10 detour at Main & 3rd"}, {"lang": "es", "text": "Desvío de la ruta 10 en Main & 3rd"}],
                "description": [{"text": "Stop C is closed. Use stop B."}],
                "communication": [{"start": ALERTS_NOW - 3600, "end": null}],
                "informed": [{"route": "10", "stop": "C"}],
                "explains": [{"kind": "edit", "id": "e1"}]
            },
            "shuttle-40": {
                "status": "published", "cause": "maintenance", "effect": "reduced_service",
                "header": [{"text": "Fewer Station Shuttle trips today"}],
                "communication": [{"start": ALERTS_NOW - 7200, "end": ALERTS_NOW + 50_000}],
                "informed": [{"route": "40", "direction": "1"}]
            },
            "t10-cancelled": {
                "status": "published", "cause": "other", "effect": "no_service",
                "header": [{"text": "The 8:00 trip on route 10 is cancelled"}],
                "communication": [{"start": ALERTS_NOW - 1800, "end": ALERTS_NOW + 3600}],
                "informed": [{"trip": "t10", "date": "2026-10-23", "start_secs": 28800}],
                "explains": [{"kind": "trip", "trip": "t10", "date": "2026-10-23", "start_secs": 28800}]
            }
        }
    }))
    .expect("the fixture alerts parse")
}

/// The first entry of an empty log.
pub fn receive_write(log: &str, writer: &str, at_ms: u64) -> transit_log::Write {
    let header = transit_log::Header {
        log: log.into(),
        seq: 1,
        writer: writer.into(),
        at_ms,
        module: format!("cap-{log}@sha256:…"),
        intent: transit_log::Intent {
            op: format!("POST /logs/{log}"),
            key: Some("device-1/1".into()),
            ..Default::default()
        },
    };
    transit_log::Write {
        checkpoint: None,
        header,
        unread: vec![],
        references: None,
        read: None,
        now: None,
        today: None,
    }
}

pub fn receive_answer(received: &transit_log::Received) -> Option<serde_json::Value> {
    let mut answer = serde_json::to_value(received).ok()?;
    answer["op"] = "receive".into();
    Some(answer)
}

/// Unix seconds for 2026-10-23 in the fixture's zone (America/Montreal).
pub const NOTES_DAY: (u64, u64) = (1_792_728_000, 1_792_814_400);

/// One note is on trip t10's 08:00 instance on 2026-10-23, edited. The other
/// is on the detour alert.
pub fn fixture_notes() -> transit_notes::Notes {
    serde_json::from_value(serde_json::json!({
        "kind": "notes", "schema_version": 1,
        "notes": {
            "n1": {
                "target": {"kind": "trip", "trip": "t10", "date": "2026-10-23", "start_secs": 28800},
                "text": "No operator. The next trip is covered.",
                "written": {"by": "dispatch-1", "at": 1_792_756_800},
                "edited": {"by": "dispatch-1", "at": 1_792_757_000}
            },
            "n2": {
                "target": {"kind": "alert", "id": "detour-10"},
                "text": "Posted. The end time follows when the crew reports.",
                "written": {"by": "comms-1", "at": 1_792_760_400}
            }
        }
    }))
    .expect("the fixture notes parse")
}

/// A stop closure with blanks, in English and Spanish.
pub fn fixture_templates() -> transit_alerts::AlertTemplates {
    serde_json::from_value(serde_json::json!({
        "kind": "alert_templates", "schema_version": 1,
        "templates": {
            "stop-closure": {
                "name": "Stop closure", "cause": "construction", "effect": "no_service",
                "header": [{"text": "The stop at {stop} is closed starting {start date}"}, {"lang": "es", "text": "La parada {stop} está cerrada"}],
                "description": [{"text": "Use {route} at the nearby stops until {end date}."}]
            }
        }
    }))
    .expect("the fixture templates parse")
}

/// A new stop at 7001 with its work order open, and a repair at 6124
/// drafted.
pub fn fixture_requests() -> transit_requests::StopRequests {
    serde_json::from_value(serde_json::json!({
        "kind": "stop_requests", "schema_version": 2,
        "requests": {
            "R-2027-014": {
                "stop": "7001", "job": "new", "status": "ordered",
                "requested_by": "Planning", "requested_on": "2027-01-04",
                "routes": ["37"], "direction": "eastbound",
                "street": "NE 104th St", "cross_street": "NE 20th Ave", "offset_ft": 60, "side": "south",
                "city": "Vancouver", "county": "Clark", "marked_by": "curb_mark",
                "locate": {"ticket": "26123456", "called_on": "2027-01-06", "utilities": ["City of Vancouver", "NW Natural"]},
                "work_order": {"system": "Maintenance", "id": "WO-88123", "status": "open", "read_on": "2027-01-07"}
            },
            "R-2027-020": {"stop": "6124", "job": "repair_improve", "status": "drafted", "note": "pad cracked at the rear door"}
        }
    }))
    .expect("the fixture requests parse")
}

/// An eight-hour platform limit, report and clear times, and overtime past
/// eight hours worked.
pub fn fixture_work_rules() -> transit_runs::WorkRules {
    serde_json::from_value::<transit_runs::WorkRules>(serde_json::json!({
        "kind": "work_rules",
        "schema_version": transit_runs::SCHEMA_VERSION,
        "name": "Fixture rules",
        "measures": {"platform": ["revenue", "deadhead", "layover"], "worked": ["report", "revenue", "deadhead", "layover", "clear"]},
        "allowances": [{"event": "pull_out", "secs": 900, "activity": "report"}, {"event": "pull_in", "secs": 900, "activity": "clear"}],
        "limits": [{"quantity": "measure", "measure": "platform", "max": 28_800}],
        "premiums": [{"on": "measure", "measure": "worked", "after_secs": 28_800, "pays": "worked", "pct": 150}]
    }))
    .expect("the fixture work rules read")
}

/// Five minutes or a tenth of the run after every trip, route 10's own rule,
/// routes 10 and 20 interlined, and one garage, with deadheads at 30 km/h.
pub fn fixture_scheduling_constraints() -> transit_runs::SchedulingConstraints {
    transit_runs::SchedulingConstraints {
        kind: Default::default(),
        schema_version: Default::default(),
        name: "Fixture constraints".into(),
        recovery: vec![
            transit_runs::RecoveryRule {
                route: None,
                service_type: None,
                min_secs: Some(300),
                pct_of_run: Some(10),
                extra: Default::default(),
            },
            transit_runs::RecoveryRule {
                route: Some("10".into()),
                service_type: None,
                min_secs: Some(240),
                pct_of_run: None,
                extra: Default::default(),
            },
        ],
        interline: vec![transit_runs::RoutePair {
            a: "10".into(),
            b: "20".into(),
            extra: Default::default(),
        }],
        relief_points: vec![],
        garages: vec![transit_runs::Garage {
            id: "yard".into(),
            name: "Fixture yard".into(),
            lat: 45.69,
            lon: -122.65,
            extra: Default::default(),
        }],
        deadhead_kph: Some(30),
        extra: Default::default(),
    }
}

mod work_rules;
pub use work_rules::*;

/// Before every fixture pick, so each publishes as a future one.
pub fn fixture_today() -> transit_date::Date {
    transit_date::Date::from_ymd_opt(2026, 1, 1).expect("a date")
}

/// The spring pick is in force on the fixture Friday. The fall pick takes
/// over after it.
pub fn fixture_plan_set() -> Vec<transit_plans::PlanInfo> {
    vec![
        transit_plans::PlanInfo {
            id: "spring".into(),
            name: "Spring 2026".into(),
            effective_from: Some("2026-03-01".into()),
        },
        transit_plans::PlanInfo {
            id: "fall".into(),
            name: "Fall 2026".into(),
            effective_from: Some("2026-09-07".into()),
        },
    ]
}

/// Route 10 kept end to end on its default pattern, five minutes added, on
/// the fixture Friday.
pub fn divert_example_draft(
    network: &transit_network::Network,
) -> Option<transit_dispatch::DiversionDraft> {
    let mut draft = transit_dispatch::DiversionDraft {
        route_id: "10".into(),
        pattern: None,
        first_kept_stop: None,
        last_kept_stop: None,
        served_stops: vec![],
        slices: vec![],
        added_mins: 5.0,
        start_date: "2026-10-23".into(),
        end_date: "2026-10-23".into(),
    };
    let stops = transit_dispatch::draft_view(network, &draft).stops;
    draft.first_kept_stop = Some(stops.first()?.stop_id.clone());
    draft.last_kept_stop = Some(stops.last()?.stop_id.clone());
    Some(draft)
}

pub fn log_record_hash(value: &serde_json::Value) -> String {
    cap_abi::canonical_hash(value).unwrap_or_else(|e| format!("unhashable: {e}"))
}

pub fn fixture_log_header(
    seq: u64,
    at_ms: u64,
    op: &str,
    key: Option<&str>,
) -> transit_log::Header {
    transit_log::Header {
        log: "stops".into(),
        seq,
        writer: "planner-3".into(),
        at_ms,
        module: "cap-log@sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .into(),
        intent: transit_log::Intent {
            op: op.into(),
            key: key.map(String::from),
            ..Default::default()
        },
    }
}

pub fn fixture_log_records() -> (Vec<transit_log::KindRecords>, Vec<transit_log::KindRecords>) {
    let stops = serde_json::to_value(
        fixture_stops()
            .checked()
            .expect("the fixture stops are valid"),
    )
    .unwrap();
    let requests = serde_json::to_value(
        fixture_requests()
            .checked()
            .expect("the fixture requests are valid"),
    )
    .unwrap();
    (
        transit_log::document_records(
            &stops,
            "stops",
            "stop",
            "stops",
            transit_stops::SCHEMA_VERSION,
        )
        .unwrap(),
        transit_log::document_records(
            &requests,
            "requests",
            "stop_request",
            "stop_requests",
            transit_requests::SCHEMA_VERSION,
        )
        .unwrap(),
    )
}

/// Entry 1 seeds the stops and requests. Entry 2 completes a request and adds
/// its stop's event.
pub fn fixture_stops_log() -> (transit_log::Checkpoint, Vec<transit_log::Entry>) {
    let (stops, requests) = fixture_log_records();
    let mut fold = transit_log::Checkpoint::empty("stops");
    let mut entries = Vec::new();
    let mut write =
        |fold: &mut transit_log::Checkpoint, header, next: &[transit_log::KindRecords]| {
            if let transit_log::Composed::Entry(e) =
                transit_log::compose(fold, header, next, &log_record_hash)
            {
                transit_log::apply(fold, &e, &log_record_hash).expect("the fixture entry applies");
                entries.push(e);
            }
        };
    write(
        &mut fold,
        fixture_log_header(1, 1_770_000_000_000, "migrate", None),
        &[stops.clone(), requests.clone()].concat(),
    );
    let (mut stops2, mut requests2) = (stops[0].clone(), requests[0].clone());
    let id = stops2
        .records
        .keys()
        .next()
        .expect("a fixture stop")
        .clone();
    if let Some(events) = stops2.records.get_mut(&id).and_then(|v| v.as_array_mut()) {
        events.push(serde_json::json!({"id": "e-repair", "on": "2027-02-20", "event": "changed", "note": "pad replaced"}));
    }
    if let Some(r) = requests2.records.values_mut().next() {
        r["status"] = serde_json::json!("completed");
        r["completed_on"] = serde_json::json!("2027-02-20");
    }
    write(
        &mut fold,
        fixture_log_header(
            2,
            1_770_000_060_000,
            "POST /logs/stops",
            Some("tablet-4/17"),
        ),
        &[stops2, requests2],
    );
    (fold, entries)
}

pub fn dispatch_fixture_edits() -> Vec<transit_sync::EditRecord> {
    let date = transit_date::Date::from_ymd_opt(2026, 10, 23).unwrap();
    vec![transit_sync::EditRecord {
        id: "e1".into(),
        diversion: Some(transit_sync::Diversion {
            id: "d1".into(),
            route_id: "10".into(),
            direction_id: None,
            first_kept_stop: "A".into(),
            last_kept_stop: "B".into(),
            skipped_stops: vec!["C".into()],
            detour: None,
            added_secs: 300,
            start_date: date,
            end_date: date,
            extra: Default::default(),
        }),
        cancel: None,
        status: transit_sync::EditStatus::Accepted,
        rev: 1,
        updated_at: 1,
        extra: Default::default(),
    }]
}

pub fn feed_bytes() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/e2e/fixtures/sample-gtfs.zip"
    ))
    .expect("the fixture feed")
}

pub fn feed_hash() -> String {
    format!("sha256:{:x}", Sha256::digest(feed_bytes()))
}

pub fn network() -> NetworkExampleContext {
    network_example_context(&feed_bytes()).expect("the fixture feed imports")
}

pub fn load(op: &str, field: &str, document: impl serde::Serialize) -> serde_json::Value {
    let mut request = serde_json::json!({ "op": op });
    request[field] = serde_json::to_value(document).expect("a fixture document serializes");
    request
}

pub fn load_feed() -> serde_json::Value {
    load("load_feed", "bytes", feed_bytes())
}

pub fn load_network(network: impl serde::Serialize) -> serde_json::Value {
    load("load_network", "network", network)
}

/// A capability and a kind that no other file names. The generator's
/// `new_names_test` takes them as it takes real ones, which shows that a new
/// name costs no line anywhere else.
pub mod new_names {
    use cap_abi::kernel::{Cap, Declared};
    use cap_abi::kinds::{Declaration, Merge, Read, Write};

    pub const KIND: Declaration = Declaration::new("test_kind", "test_kind", "", "fixtures")
        .stored(
            "test_kind",
            Write::Route,
            "POST /test-kind, POST /logs/test_log",
            &["test_kind"],
            Merge::Log,
            Read::Open,
        )
        .checked("testcap", "validate")
        .logged("test_log", &[("test_record", "records")])
        .received("testcap")
        .posted("/test-kind", "validate", "test_kind_hash", &[]);

    fn json(text: &str) -> String {
        let op = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|v| v["op"].as_str().map(str::to_string));
        match op.as_deref() {
            Some(op @ ("validate" | "receive")) => format!(r#"{{"ok":{{"op":"{op}"}}}}"#),
            _ => r#"{"error":{"message":"unknown op"}}"#.to_string(),
        }
    }

    fn bytes(_: u32, _: &[u8]) -> String {
        r#"{"error":{"message":"no bulk ops"}}"#.to_string()
    }

    pub static CAP: Cap = Cap {
        id: "testcap",
        json,
        bytes,
        documents: &[],
    };

    pub fn declared() -> Declared {
        Declared {
            id: "testcap",
            invoke: json,
            invoke_bytes: bytes,
            ops: vec!["validate".into(), "receive".into()],
            documents: vec![],
            consumes: &[],
            speaks: &[],
            migrates: &[],
            docs: "# testcap\n\nChecks and receives the test kind.\n\n## validate\n\nChecks a test document.\n\n## receive\n\nWrites the test log.\n",
        }
    }
}
