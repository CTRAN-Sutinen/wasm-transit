//! The dispatch laws, model-native (the day-view laws): the day is
//! `materialize(net, date)`, operator edits patch THE DAY, boards
//! and chains read the patched day. No GTFS substrate exists here to
//! accidentally test against — the network below is authored model
//! code, the same vocabulary the library speaks.
//!
//! The laws:
//!
//! 1. the edit document patches the materialized day: skipped stops
//!    lose service, added time shifts everything from the last
//!    anchor onward, and nothing else moves;
//! 2. the input day is never mutated (before/after comparison stays
//!    trivial — the base-feed immutability law, model edition);
//! 3. the station board pools the walking group, rows tagged with
//!    their concrete bay;
//! 4. the block chain is the AUTHORED runcut; a diverted trip eats
//!    its next layover (the connection-health ripple);
//! 5. edit validation reports against the NETWORK, invalid records
//!    reported, never rejected.

use chrono::NaiveDate;
use transit_dispatch::{block_chain, board, effective_day, patch_day, report_against_network, station_board};
use transit_network::{
    Block, CalendarRule, DateRange, Layover, MaterializedTrip, Network, Pattern, PatternSchedule,
    PatternStop, Route, Schedule, ServiceType, Stop, TimingProfile, TripRef, WeekdaySet,
};
use transit_sync::{Diversion, EditRecord, EditStatus};

const DATE: &str = "2026-10-23"; // a Friday inside the fixture year

fn date() -> NaiveDate {
    NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap()
}

fn stop(id: &str, name: &str, parent: Option<&str>) -> Stop {
    Stop {
        name: name.into(),
        lat: 45.5,
        lon: -122.5,
        parent: parent.map(|p| p.into()),
        transfer: false,
    }
}

fn timepoint(id: &str, distance_m: f64) -> PatternStop {
    PatternStop { stop: id.into(), timepoint: true, distance_m }
}

fn pattern(route: &str, stops: Vec<PatternStop>, at_timepoints: Vec<u32>) -> Pattern {
    Pattern {
        route: route.into(),
        name: format!("route {route}"),
        path: None,
        stops,
        timing: vec![TimingProfile { start_secs: 0, end_secs: 172_800, at_timepoints }],
    }
}

/// The authored test network: route 10 A→C→B, route 20 C→B→A, a
/// shuttle between two bays of one station, one weekday service
/// type, and a block chaining the 08:00 trip of route 10 to the
/// 08:30 trip of route 20 (layover 600 s at B).
fn network() -> Network {
    let mut stops = std::collections::BTreeMap::new();
    stops.insert("A".to_string(), stop("A", "Main & 1st", None));
    stops.insert("B".to_string(), stop("B", "Main & 2nd", None));
    stops.insert("C".to_string(), stop("C", "Main & 3rd", None));
    stops.insert("STN".to_string(), stop("STN", "Main Station", None));
    stops.insert("SA".to_string(), stop("SA", "Bay A", Some("STN")));
    stops.insert("SB".to_string(), stop("SB", "Bay B", Some("STN")));

    let mut routes = std::collections::BTreeMap::new();
    routes.insert("10".to_string(), Route { name: "10".into(), mode: transit_network::Mode::Bus, color: None });
    routes.insert("20".to_string(), Route { name: "20".into(), mode: transit_network::Mode::Bus, color: None });
    routes.insert("40".to_string(), Route { name: "Circulator".into(), mode: transit_network::Mode::Bus, color: None });

    let mut patterns = std::collections::BTreeMap::new();
    patterns.insert(
        "p10".to_string(),
        pattern("10", vec![timepoint("A", 0.0), timepoint("C", 1000.0), timepoint("B", 2000.0)], vec![0, 600, 1200]),
    );
    patterns.insert(
        "p20".to_string(),
        pattern("20", vec![timepoint("C", 0.0), timepoint("B", 1000.0), timepoint("A", 2000.0)], vec![0, 600, 1200]),
    );
    patterns.insert(
        "p40".to_string(),
        pattern("40", vec![timepoint("SA", 0.0), timepoint("SB", 500.0)], vec![0, 300]),
    );

    let mut service_types = std::collections::BTreeMap::new();
    service_types.insert(
        "WK".to_string(),
        ServiceType {
            name: "Weekday".into(),
            rule: CalendarRule {
                days: WeekdaySet::weekdays(),
                ranges: vec![DateRange {
                    start: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                    end: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
                }],
                exceptions: vec![],
            },
        },
    );
    service_types.insert(
        "SUN".to_string(),
        ServiceType {
            name: "Sunday".into(),
            rule: CalendarRule { days: WeekdaySet::none(), ranges: vec![], exceptions: vec![] },
        },
    );

    let mut schedules = std::collections::BTreeMap::new();
    schedules.insert(
        "s10".to_string(),
        PatternSchedule { pattern: "p10".into(), service_type: "WK".into(), schedule: Schedule::ExactTimes { starts: vec![28_800] } },
    );
    schedules.insert(
        "s20".to_string(),
        PatternSchedule { pattern: "p20".into(), service_type: "WK".into(), schedule: Schedule::ExactTimes { starts: vec![30_600] } },
    );
    schedules.insert(
        "s20sun".to_string(),
        PatternSchedule { pattern: "p20".into(), service_type: "SUN".into(), schedule: Schedule::ExactTimes { starts: vec![30_600] } },
    );
    schedules.insert(
        "s40".to_string(),
        PatternSchedule { pattern: "p40".into(), service_type: "WK".into(), schedule: Schedule::ExactTimes { starts: vec![29_700, 30_600] } },
    );

    let mut blocks = std::collections::BTreeMap::new();
    blocks.insert(
        "B1".to_string(),
        Block {
            trips: vec![TripRef { schedule: "s10".into(), start_secs: 28_800 }, TripRef { schedule: "s20".into(), start_secs: 30_600 }],
            layovers: vec![Layover { after_trip: 0, secs: 600, at: Some("C".into()) }],
        },
    );
    blocks.insert(
        "B2".to_string(),
        Block {
            trips: vec![TripRef { schedule: "s10".into(), start_secs: 28_800 }, TripRef { schedule: "s20sun".into(), start_secs: 30_600 }],
            layovers: vec![Layover { after_trip: 0, secs: 600, at: None }],
        },
    );

    Network {
        name: "Test network".into(),
        plan: Default::default(),
        anchor: Default::default(),
        stops,
        routes,
        patterns,
        service_types,
        schedules,
        blocks,
    }
}

/// The test diversion: route 10 keeps A→B, skips C, +300 s.
fn diversion(added_secs: u32) -> Diversion {
    Diversion {
        id: "d1".into(),
        route_id: "10".into(),
        direction_id: None,
        first_kept_stop: "A".into(),
        last_kept_stop: "B".into(),
        skipped_stops: vec!["C".into()],
        detour: None,
        added_secs,
        start_date: date(),
        end_date: date(),
    }
}

fn record(status: EditStatus, added_secs: u32) -> EditRecord {
    EditRecord { id: "e1".into(), diversion: diversion(added_secs), status, rev: 1, updated_at: 1 }
}

fn find_trip<'a>(day: &'a [MaterializedTrip], schedule: &str) -> &'a MaterializedTrip {
    day.iter().find(|t| t.schedule == schedule).unwrap()
}

#[test]
fn edits_patch_the_day_skip_shift_and_only_the_match() {
    let net = network();
    let base = transit_network::materialize(&net, date()).unwrap();
    assert_eq!(base.len(), 4, "s10, s20, and two s40 trips");

    let patched = patch_day(&base, &[diversion(300)]);

    // The diverted trip: C loses its stop time entirely, B and every
    // stop from the last anchor onward shift +300, A does not move.
    let t10 = find_trip(&patched, "s10");
    let stops: Vec<&str> = t10.stop_times.iter().map(|st| st.stop.as_str()).collect();
    assert_eq!(stops, vec!["A", "B"], "the skipped stop loses service");
    let a = t10.stop_times.iter().find(|st| st.stop == "A").unwrap();
    let b = t10.stop_times.iter().find(|st| st.stop == "B").unwrap();
    assert_eq!(a.departure_secs, 28_800, "before the last anchor: untouched");
    assert_eq!(b.arrival_secs, 28_800 + 1200 + 300, "the detour running time lands on the last anchor onward");

    // Another route's trip with the same stops in view: untouched.
    let t20 = find_trip(&patched, "s20");
    assert_eq!(t20.stop_times.len(), 3, "route 20 keeps its whole pattern");
    let c20 = t20.stop_times.iter().find(|st| st.stop == "C").unwrap();
    assert_eq!(c20.departure_secs, 30_600, "unmatched trips do not move");

    // A diversion whose anchors are not in trip order does not match.
    let reversed = Diversion {
        first_kept_stop: "B".into(),
        last_kept_stop: "A".into(),
        ..diversion(300)
    };
    let untouched = patch_day(&base, &[reversed]);
    let t10 = find_trip(&untouched, "s10");
    assert_eq!(t10.stop_times.len(), 3, "anchors out of order never match");
}

#[test]
fn the_day_is_never_mutated() {
    let net = network();
    let base = transit_network::materialize(&net, date()).unwrap();
    let before = base.clone();
    let _ = patch_day(&base, &[diversion(300)]);
    assert_eq!(base, before, "the base day is immutable (before/after comparison stays trivial)");
}

#[test]
fn boards_read_the_patched_day() {
    let net = network();
    let records = vec![record(EditStatus::Accepted, 300)];
    let day = effective_day(&net, date(), &records).unwrap();

    // B's board: the diverted trip runs 5 minutes late.
    let rows = board(&net, &day, "B", 0, 10);
    let by_route: std::collections::BTreeMap<&str, u32> =
        rows.iter().map(|r| (r.route.as_str(), r.departure_secs)).collect();
    assert_eq!(by_route.get("10"), Some(&(28_800 + 1200 + 300)), "the diverted trip at B");
    assert_eq!(by_route.get("20"), Some(&(30_600 + 600)), "route 20 on time");
    assert_eq!(by_route.get("40"), None, "the shuttle never serves B");

    // C loses the diverted route's service — the skipped stop shows
    // its emptiness (the diversion law). Route 20 still starts
    // there; route 10's rows are gone.
    let at_c = board(&net, &day, "C", 0, 10);
    assert!(
        at_c.iter().all(|r| r.route != "10"),
        "the skipped stop loses the diverted route's service: {at_c:?}"
    );
    assert_eq!(at_c.len(), 1, "route 20 still serves C");

    // The row speaks model vocabulary: trip identity, route name,
    // concrete stop, destination.
    let row = &rows[0];
    assert_eq!(row.schedule, "s10");
    assert_eq!(row.start_secs, 28_800);
    assert_eq!(row.route_name, "10");
    assert_eq!(row.destination, "Main & 2nd");

    // from_secs windows the board; limit truncates it.
    assert!(board(&net, &day, "B", 40_000, 10).is_empty());
    assert_eq!(board(&net, &day, "B", 0, 1).len(), 1);
}

#[test]
fn the_station_board_pools_the_walking_group() {
    let net = network();
    let day = transit_network::materialize(&net, date()).unwrap();

    // The station pin answers its bays' board, rows tagged with the
    // concrete bay; any member of the group answers the same board.
    let from_pin = station_board(&net, &day, "STN", 0, 10);
    let from_bay = station_board(&net, &day, "SA", 0, 10);
    assert_eq!(from_pin, from_bay, "every member of the group answers the same board");
    assert_eq!(from_pin.len(), 4, "two shuttle trips, two bays each");
    assert!(from_pin.iter().all(|r| r.stop == "SA" || r.stop == "SB"), "rows name their bay");
    // Sorted by time across bays.
    let times: Vec<u32> = from_pin.iter().map(|r| r.departure_secs).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted);
}

#[test]
fn the_block_chain_is_the_authored_runcut_with_the_ripple() {
    let net = network();
    let schedule_day = transit_network::materialize(&net, date()).unwrap();
    let chain = block_chain(&net, &schedule_day, "B1");
    assert_eq!(chain.len(), 2, "the authored chain, in order");
    assert_eq!(chain[0].schedule, "s10");
    assert_eq!(chain[0].layover_after_secs, Some(600), "the authored layover");
    assert_eq!(chain[0].layover_remaining_secs, Some(600), "no edits: the layover stands");
    assert_eq!(chain[1].start_secs, 30_600);
    assert!(chain.iter().all(|t| t.runs), "everything runs on the fixture Friday");

    // The ripple: the diverted trip ends +300 — the 10-minute layover
    // has 4 minutes left (the connection-health read).
    let records = vec![record(EditStatus::Accepted, 300)];
    let day = effective_day(&net, date(), &records).unwrap();
    let ripped = block_chain(&net, &day, "B1");
    assert_eq!(ripped[0].end_secs, Some(28_800 + 1200 + 300), "the effective end");
    assert_eq!(ripped[0].layover_remaining_secs, Some(300), "the diverted end eats the layover (600 - 300)");

    // A connection eaten outright goes negative — the dispatcher's
    // alarm, not a silent clamp.
    let eaten = vec![record(EditStatus::Accepted, 900)];
    let day = effective_day(&net, date(), &eaten).unwrap();
    let chain = block_chain(&net, &day, "B1");
    assert_eq!(chain[0].layover_remaining_secs, Some(-300), "an eaten connection is negative");

    // Unknown blocks answer empty, never error.
    assert!(block_chain(&net, &day, "nope").is_empty());

    // A trip of the chain that does not run on the date stays in the
    // chain, marked — the authored work, the hole visible.
    let b2 = block_chain(&net, &schedule_day, "B2");
    assert!(b2[0].runs, "s10 runs on the Friday");
    assert!(!b2[1].runs, "the Sunday schedule does not");
    assert_eq!(b2[1].end_secs, None);
}

#[test]
fn validation_reports_against_the_network() {
    let net = network();

    // Valid: reported valid with the touched-trip count.
    let reports = report_against_network(&net, &[record(EditStatus::Accepted, 300)]);
    assert_eq!(reports.len(), 1);
    assert!(reports[0].valid, "valid against the model: {:?}", reports);
    assert_eq!(reports[0].affected_trips, 1, "one s10 trip carries both anchors");

    // Unknown route and unknown stops: reported invalid, one report
    // per record, input order.
    let mut unknown_route = record(EditStatus::Accepted, 300);
    unknown_record(&mut unknown_route, |d| d.route_id = "ghost".into());
    let mut unknown_stop = record(EditStatus::Accepted, 300);
    unknown_record(&mut unknown_stop, |d| d.first_kept_stop = "ghost".into());
    let mut anchor_skip = record(EditStatus::Accepted, 300);
    unknown_record(&mut anchor_skip, |d| d.skipped_stops = vec!["A".into()]);
    let reports = report_against_network(
        &net,
        &[unknown_route, unknown_stop, anchor_skip, record(EditStatus::Accepted, 60)],
    );
    assert_eq!(reports.len(), 4);
    assert!(!reports[0].valid, "unknown route");
    assert!(reports[0].error.as_deref().unwrap().contains("unknown route"));
    assert!(!reports[1].valid, "unknown anchor");
    assert!(!reports[2].valid, "a skipped stop must not be an anchor");
    assert!(reports[3].valid, "the good record among the bad");
}

fn unknown_record(record: &mut EditRecord, edit: impl FnOnce(&mut Diversion)) {
    record.id = format!("{}-x", record.id);
    edit(&mut record.diversion);
}

#[test]
fn retired_never_patches_but_a_draft_previews() {
    let net = network();
    // Retired is the tombstone: the schedule answers.
    let day = effective_day(&net, date(), &[record(EditStatus::Retired, 300)]).unwrap();
    let t10 = find_trip(&day, "s10");
    assert_eq!(t10.stop_times.len(), 3, "a retired edit never applies");

    // A draft previews locally (the dispatcher's own document, run
    // locally — the sync contract).
    let day = effective_day(&net, date(), &[record(EditStatus::Draft, 300)]).unwrap();
    let t10 = find_trip(&day, "s10");
    assert_eq!(t10.stop_times.len(), 2, "a draft previews the edit");

    // Another date: not covered, the schedule answers.
    let other = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
    let day = effective_day(&net, other, &[record(EditStatus::Accepted, 300)]).unwrap();
    let t10 = find_trip(&day, "s10");
    assert_eq!(t10.stop_times.len(), 3, "the edit does not cover the 30th");
}
