//! Planner-analysis regression tests over production-shaped feeds
//! (found importing C-TRAN; the fixtures import through the codec,
//! the boundary). The property laws live in analysis_prop_test.rs;
//! these pin the numbers the planner's design axis reads.

use std::io::Write;

use transit_analysis::{
    network_stats, parse_pop_layer, service_type_detail, service_type_overview, type_coverage, CoverageWindow,
};

const DATE: &str = "2026-10-23"; // Friday: WK + HOL both run

/// A production-shaped feed (see the codec's import regression
/// suite for the full commentary): trip ids that do not sort by
/// start time, a duplicated start (dedup), and per-trip run-time
/// variation inside one pattern.
fn production_zip() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };

    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n");
    add(
        &mut w,
        "stops.txt",
        "stop_id,stop_name,stop_lat,stop_lon\nA,Main & 1st,45.500,-73.500\nB,Main & 2nd,45.510,-73.510\nC,Main & 3rd,45.520,-73.520\n",
    );
    add(
        &mut w,
        "trips.txt",
        "route_id,service_id,trip_id,direction_id,block_id\n10,WK,t1,0,B1\n10,WK,t3,0,\n10,WK,t5,0,B1\n10,WK,t9,0,B1\n10,WK,t7,0,\n",
    );
    add(
        &mut w,
        "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
         t9,06:00:00,06:00:00,A,1\nt9,06:10:00,06:10:00,B,2\nt9,06:30:00,06:30:00,C,3\n\
         t1,08:00:00,08:00:00,A,1\nt1,08:08:00,08:08:00,B,2\nt1,08:20:00,08:20:00,C,3\n\
         t3,08:00:00,08:00:00,A,1\nt3,08:08:00,08:08:00,B,2\nt3,08:20:00,08:20:00,C,3\n\
         t5,08:22:00,08:22:00,A,1\nt5,08:30:00,08:30:00,B,2\nt5,08:42:00,08:42:00,C,3\n\
         t7,09:00:00,09:00:00,A,1\nt7,09:10:00,09:10:00,B,2\n",
    );
    add(
        &mut w,
        "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWK,1,1,1,1,1,0,0,20260101,20261231\n",
    );
    w.finish().unwrap().into_inner()
}

fn import_production() -> transit_network::Network {
    let gtfs = transit_core::Gtfs::from_zip_bytes(&production_zip()).unwrap();
    transit_codec::gtfs_to_network(&gtfs).network
}

/// Three population points: on stop A, on stop C, and far from the
/// network. Property spelled "POP" (auto-detected on load).
fn pop_layer_geojson() -> Vec<u8> {
    br#"{"type":"FeatureCollection","features":[
        {"type":"Feature","geometry":{"type":"Point","coordinates":[-73.500,45.500]},"properties":{"POP":100}},
        {"type":"Feature","geometry":{"type":"Point","coordinates":[-73.520,45.520]},"properties":{"POP":50}},
        {"type":"Feature","geometry":{"type":"Point","coordinates":[-73.400,45.500]},"properties":{"POP":30}}
    ]}"#
    .to_vec()
}

#[test]
fn stats_report_route_and_stop_activity() {
    // The planner's read view: activity on one service date, from the
    // document (no materialization needed). On the production-shaped
    // fixture (DATE = Friday, WK runs): 4 trips survive the
    // duplicate-start dedup (t1/t3 share 08:00), two patterns, three
    // stops, every stop on route 10.
    let network = import_production();
    let date = chrono::NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap();
    let stats = network_stats(&network, date);
    assert_eq!(stats.date, DATE);
    assert_eq!(stats.trips, 4); // t9, t1, t5 (dedup), t7
    assert_eq!(stats.active_routes, 1);
    assert_eq!(stats.active_stops, 3);
    assert_eq!(stats.first_start_secs, Some(6 * 3600));
    assert_eq!(stats.last_arrival_secs, Some(9 * 3600 + 10 * 60));
    // Run times as the MODEL materializes them — each trip's own
    // (the import preserves per-trip run time as pattern variants):
    // t9 30 min, t1 20 (t3 dedups onto it), t5 20, t7 10 — 4800 s
    // total. The old import collapsed every A→C trip to the
    // shortest member's 20 minutes and reported 4200 s.
    assert!((stats.revenue_hours - 4800.0 / 3600.0).abs() < 1e-9);
    let route = &stats.routes[0];
    assert_eq!(route.route_id, "10");
    assert_eq!(route.trips, 4);
    assert_eq!(route.patterns, 3);
    assert_eq!(route.stops, 3);
    assert_eq!(route.first_start_secs, Some(6 * 3600));
    assert_eq!(route.last_arrival_secs, Some(9 * 3600 + 10 * 60));
    // The busiest stop is A (06:00/08:00/08:22/09:00 — t7's A→B
    // departs A too; B is only ever passed or ended at). Gaps 7200,
    // 1320, 2280 → the median 2280 s (38 min). The old per-pattern
    // span average said 4260 s, and a clustered pattern could say
    // anything lower — QAF-41.
    assert_eq!(route.headway_stop.as_deref(), Some("A"));
    assert_eq!(route.typical_headway_secs, Some(2280));
    // The route's own revenue hours: all of the day's runtime here
    // (30 + 20 + 20 + 10 min = 4800 s), the per-route cost basis.
    assert!((route.revenue_hours - 4800.0 / 3600.0).abs() < 1e-9);
    // QAF-21: the other cost basis — revenue MILES, straight off the
    // patterns' authored distances (no shape_dist in the fixture, so
    // the codec's 500 m per stop applies): three A→C→B trips at
    // 1000 m and one A→B at 500 m = 3500 m.
    assert!((stats.revenue_miles - 3500.0 / 1609.344).abs() < 1e-9);
    assert!((route.revenue_miles - 3500.0 / 1609.344).abs() < 1e-9);

    let by_id = |id: &str| stats.stops.iter().find(|s| s.stop_id == id).expect(id);
    let a = by_id("A");
    assert_eq!((a.departures, a.routes), (4, 1));
    assert_eq!(a.first_arrival_secs, Some(6 * 3600));
    assert_eq!(a.last_arrival_secs, Some(9 * 3600));
    assert_eq!(a.typical_headway_secs, Some((9 * 3600 - 6 * 3600) / 3));
    let c = by_id("C");
    assert_eq!(c.departures, 3);
    // C arrivals as materialized: 06:30/08:20/08:42 — each trip's
    // own feed times (t9's 06:30 survives; the old import collapsed
    // it to the 20-minute variant's 06:20).
    assert_eq!(c.typical_headway_secs, Some(((8 * 3600 + 42 * 60) - (6 * 3600 + 30 * 60)) / 2));
}

#[test]
fn service_type_ops_expose_the_design_axis() {
    // The planner designs SERVICE TYPES, not calendar dates: the
    // overview lists every type on its representative day, the
    // detail expands one type into everything that runs under it.
    let network = import_production();

    let types = service_type_overview(&network);
    assert_eq!(types.len(), 1, "one service key (WK) in the fixture");
    assert_eq!(types[0].service_type, "WK");
    assert_eq!(types[0].date, "2026-01-01", "first date the rule runs (a Thursday)");
    assert_eq!(types[0].trips, 4);
    assert_eq!(types[0].active_routes, 1);
    assert_eq!(types[0].active_stops, 3);
    assert!((types[0].revenue_hours - 4800.0 / 3600.0).abs() < 1e-9);
    assert!((types[0].revenue_miles - 3500.0 / 1609.344).abs() < 1e-9);

    let detail = service_type_detail(&network, "WK").expect("detail");
    assert_eq!(detail.trips, 4);
    assert_eq!(detail.routes.len(), 1, "route 10 runs; zero-trip routes filtered out");
    assert_eq!(detail.routes[0].typical_headway_secs, Some(2280));
    assert!((detail.revenue_miles - 3500.0 / 1609.344).abs() < 1e-9);
    assert_eq!(detail.stops.len(), 3, "A, B and C are all served");
    let schedule_trips: std::collections::BTreeMap<&str, u32> = detail
        .pattern_trips
        .iter()
        .map(|s| (s.pattern_trips.as_str(), s.trips))
        .collect();
    // Three pattern variants: t7's A→B, the 20-minute A→C trips,
    // and t9's 30-minute A→C (run time is part of the pattern
    // identity — the import preserves each trip's own times).
    assert_eq!(schedule_trips["WK:pat-10-0-1"], 2);
    assert_eq!(schedule_trips["WK:pat-10-0-2"], 1);
    assert_eq!(schedule_trips["WK:pat-10-0-0"], 1);

    // Coverage per service type: same layer, WK on its day.
    let layer = parse_pop_layer(&pop_layer_geojson(), None).expect("layer parses");
    let coverage = type_coverage(&network, &layer, 400.0, 3600, 4, None);
    assert_eq!(coverage.len(), 1);
    assert_eq!(coverage[0].service_type, "WK");
    assert!((coverage[0].served_pct - 150.0 / 180.0 * 100.0).abs() < 1e-6);
    assert!((coverage[0].frequent_pct - 100.0 / 180.0 * 100.0).abs() < 1e-6);
}

#[test]
fn frequency_in_a_window_is_the_worst_wait() {
    // QAF-42: the whole-day average hides a frequent window. Stop A
    // departs 06:00, 08:00, 08:22, 09:00 — a 60-min average, so a
    // 40-min frequency test fails on the day; between 08:00 and 09:00
    // nobody waits more than 38 minutes, so it passes in that window.
    // The window counts its edges: from 07:00 the first wait is 60.
    let network = import_production();
    let layer = parse_pop_layer(&pop_layer_geojson(), None).expect("layer parses");
    let frequent_pct = |window: Option<CoverageWindow>| {
        type_coverage(&network, &layer, 400.0, 2400, 2, window)[0].frequent_pct
    };
    let on_a = 100.0 / 180.0 * 100.0;
    assert_eq!(frequent_pct(None), 0.0, "the whole-day average (60 min) is not 40-min frequent");
    let hour = CoverageWindow { start_secs: 8 * 3600, end_secs: 9 * 3600 };
    assert!((frequent_pct(Some(hour)) - on_a).abs() < 1e-6, "08:00–09:00: worst wait 38 min ≤ 40");
    let wider = CoverageWindow { start_secs: 7 * 3600, end_secs: 9 * 3600 };
    assert_eq!(frequent_pct(Some(wider)), 0.0, "07:00–09:00: a rider at 07:00 waits 60 min");

    // C is only ever a trip's last stop: served (riders alight), but
    // never frequent — an arrival is not a departure.
    let all_day = CoverageWindow { start_secs: 0, end_secs: 30 * 3600 };
    let c = type_coverage(&network, &layer, 400.0, 30 * 3600, 1, Some(all_day));
    assert!((c[0].served_pct - 150.0 / 180.0 * 100.0).abs() < 1e-6, "A and C both served");
    assert!((c[0].frequent_pct - on_a).abs() < 1e-6, "only A departs anything");
}

#[test]
fn a_proposal_compares_route_by_route_against_its_baseline() {
    // "What does this change do?": route 10 runs its last trip on to
    // 21:00 every 3 h (12:00 … 21:00) — the only changed row.
    let base = import_production();
    let proposal = transit_network::apply_edits(
        &base,
        &[transit_network::NetworkEdit::SetSpan {
            route: "10".into(),
            direction: Some("0".into()),
            service_type: "WK".into(),
            first_secs: None,
            last_secs: Some(21 * 3600),
            headway_secs: Some(3 * 3600),
        }],
    )
    .unwrap();
    let cmp = transit_analysis::service_type_compare(&base, &proposal, "WK").unwrap();
    assert_eq!((cmp.base.trips, cmp.base.routes), (4, 1));
    assert!(cmp.proposal.trips > cmp.base.trips);
    assert!(cmp.proposal.revenue_hours > cmp.base.revenue_hours);
    assert_eq!(cmp.routes.len(), 1);
    let row = &cmp.routes[0];
    assert_eq!(row.route_id, "10");
    assert_eq!(row.base.as_ref().unwrap().trips, 4);
    assert_eq!(row.proposal.as_ref().unwrap().trips, cmp.proposal.trips);
    assert_eq!(cmp.unchanged_routes, 0);

    // A document against itself: nothing changed.
    let same = transit_analysis::service_type_compare(&base, &base, "WK").unwrap();
    assert!(same.routes.is_empty());
    assert_eq!(same.unchanged_routes, 1);
    // An unknown type answers empty, never an error — the lookup
    // convention (QAF-19): errors are for hash addresses.
    assert!(transit_analysis::service_type_compare(&base, &proposal, "ghost").is_none());
}
