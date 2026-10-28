//! Analysis laws, as property tests with independent oracles.
//!
//! `analysis.rs` is what the planner persona READS — headways,
//! spans, coverage. Its failure mode is silent plausibility: wrong
//! numbers that neither panic nor look wrong. These laws hold for
//! every generated network (the adversarial import generator, as in
//! `codec_prop_test.rs`) and every generated population layer:
//!
//! * **Stats oracle** — every number `network_stats` reports is
//!   recomputed from `materialize` output with independent
//!   aggregation code: trip counts, first/last times, revenue hours,
//!   per-route and per-stop activity, typical headways.
//! * **Coverage monotonicity and bounds** — a bigger radius never
//!   serves less population; served/frequent population stay
//!   within [0, total]; the served-stop counts are radius-
//!   independent.
//! * **Cross-function agreement** — `type_coverage` (the design
//!   axis) agrees with `coverage` (the calendar view) on the same
//!   representative date.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use chrono::{Duration, NaiveDate};
use proptest::prelude::*;

const EPOCH: NaiveDate = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

// ---------------------------------------------------------------------
// Network generator (the adversarial import generator, as in
// import_prop_test.rs / codec_prop_test.rs)
// ---------------------------------------------------------------------

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
                .collect::<BTreeSet<_>>()
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

// ---------------------------------------------------------------------
// Population layer generator
// ---------------------------------------------------------------------

/// One population point: anchored near a network stop (jittered) or
/// far away (never served).
#[derive(Clone, Debug)]
enum PopSpec {
    Near { stop: u8, jit_lat: f64, jit_lon: f64, pop: u32 },
    Far { pop: u32 },
}

fn pop_point(net: &transit_network::Network, p: &PopSpec) -> (f64, f64, f64) {
    match p {
        PopSpec::Near { stop, jit_lat, jit_lon, pop } => {
            let s = &net.stops[&format!("s{stop}")];
            (s.lat + jit_lat * 0.001, s.lon + jit_lon * 0.001, *pop as f64)
        }
        PopSpec::Far { pop } => (46.0, -74.0, *pop as f64),
    }
}

fn build_layer(net: &transit_network::Network, points: &[PopSpec]) -> Vec<u8> {
    let mut features = Vec::new();
    for p in points {
        let (lat, lon, pop) = pop_point(net, p);
        features.push(format!(
            "{{\"type\":\"Feature\",\"properties\":{{\"pop\":{pop}}},\"geometry\":{{\"type\":\"Point\",\"coordinates\":[{lon},{lat}]}}}}"
        ));
    }
    format!("{{\"type\":\"FeatureCollection\",\"features\":[{}]}}", features.join(",")).into_bytes()
}

fn pop_spec() -> impl Strategy<Value = PopSpec> {
    proptest::prop_oneof![
        4 => (
            0u8..6,
            -1.0f64..1.0,
            -1.0f64..1.0,
            proptest::num::u32::ANY,
        ).prop_map(|(stop, jit_lat, jit_lon, pop)| PopSpec::Near { stop, jit_lat, jit_lon, pop }),
        1 => proptest::num::u32::ANY.prop_map(|pop| PopSpec::Far { pop }),
    ]
}

fn spec_and_layer() -> impl Strategy<Value = (transit_network::Network, Vec<PopSpec>)> {
    proptest::collection::vec(gen_trip(), 0..=14).prop_flat_map(|trips| {
        proptest::collection::vec(pop_spec(), 1..=20).prop_map(move |points| {
            let gtfs = transit_core::Gtfs::from_zip_bytes(&gen_feed(&trips)).expect("parse");
            let net = transit_codec::gtfs_to_network(&gtfs).network;
            (net.clone(), points)
        })
    })
}

// ---------------------------------------------------------------------
// The stats oracle: aggregation over materialize output, no
// implementation code
// ---------------------------------------------------------------------

struct StopOracle {
    departures: u32,
    routes: BTreeSet<String>,
    first: Option<u32>,
    last: Option<u32>,
}

struct PatternOracle {
    starts: Vec<u32>,
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Stats oracle: every number network_stats reports matches an
    /// independent recomputation from the materialized trips.
    #[test]
    fn stats_match_the_materialize_oracle(
        trips in proptest::collection::vec(gen_trip(), 0..=14),
    ) {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let net = transit_codec::gtfs_to_network(&gtfs).network;

        for day in [0i64, 3, 400] {
            let date = EPOCH + Duration::days(day);
            let stats = transit_analysis::network_stats(&net, date);

            // Oracle: aggregate the materialized trips directly.
            let materialized = transit_network::materialize(&net, date).expect("materialize");
            let mut trips = 0u32;
            let mut first_start = None;
            let mut last_arrival = None;
            let mut runtime = 0u64;
            let mut route_trips: BTreeMap<String, u32> = BTreeMap::new();
            let mut route_first: BTreeMap<String, u32> = BTreeMap::new();
            let mut route_last: BTreeMap<String, u32> = BTreeMap::new();
            let mut route_patterns: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            let mut route_runtime: BTreeMap<String, u64> = BTreeMap::new();
            // (route, stop) → departures (every visit but the trip's last).
            let mut route_departures: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();
            let mut stop_oracles: BTreeMap<String, StopOracle> = BTreeMap::new();
            for trip in &materialized {
                let route = net.patterns[&trip.pattern].route.clone();
                let end = trip.stop_times.last().expect("trip has stops");
                trips += 1;
                *route_trips.entry(route.clone()).or_default() += 1;
                route_first.entry(route.clone()).and_modify(|f| *f = (*f).min(trip.start_secs)).or_insert(trip.start_secs);
                route_last.entry(route.clone()).and_modify(|l| *l = (*l).max(end.arrival_secs)).or_insert(end.arrival_secs);
                route_patterns.entry(route.clone()).or_default().insert(trip.pattern.clone());
                *route_runtime.entry(route.clone()).or_default() += end.arrival_secs.saturating_sub(trip.start_secs) as u64;
                for st in trip.stop_times.iter().rev().skip(1) {
                    route_departures.entry((route.clone(), st.stop.clone())).or_default().push(st.departure_secs);
                }
                first_start = Some(first_start.map_or(trip.start_secs, |f: u32| f.min(trip.start_secs)));
                last_arrival = Some(last_arrival.map_or(end.arrival_secs, |l: u32| l.max(end.arrival_secs)));
                runtime += end.arrival_secs.saturating_sub(trip.start_secs) as u64;
                for st in &trip.stop_times {
                    let o = stop_oracles.entry(st.stop.clone()).or_insert(StopOracle {
                        departures: 0, routes: BTreeSet::new(), first: None, last: None,
                    });
                    o.departures += 1;
                    o.routes.insert(route.clone());
                    o.first = Some(o.first.map_or(st.arrival_secs, |f: u32| f.min(st.arrival_secs)));
                    o.last = Some(o.last.map_or(st.arrival_secs, |l: u32| l.max(st.arrival_secs)));
                }
            }

            prop_assert_eq!(stats.trips, trips, "trips on {:?}", date);
            prop_assert_eq!(stats.first_start_secs, first_start, "first start on {:?}", date);
            prop_assert_eq!(stats.last_arrival_secs, last_arrival, "last arrival on {:?}", date);
            prop_assert_eq!(stats.active_routes, route_trips.len() as u32, "active routes on {:?}", date);
            prop_assert_eq!(stats.active_stops, stop_oracles.len() as u32, "active stops on {:?}", date);
            prop_assert!(close(stats.revenue_hours, runtime as f64 / 3600.0, 1e-9),
                "revenue hours on {:?}", date);

            // Per-route: trips, first, last, pattern count, headway.
            for route in &stats.routes {
                let Some(&trips_r) = route_trips.get(&route.route_id) else {
                    prop_assert_eq!(route.trips, 0, "zero-trip route {} reports trips", route.route_id);
                    continue;
                };
                prop_assert_eq!(route.trips, trips_r, "route {} trips", route.route_id);
                prop_assert_eq!(route.first_start_secs, Some(*route_first.get(&route.route_id).unwrap()));
                prop_assert_eq!(route.last_arrival_secs, Some(*route_last.get(&route.route_id).unwrap()));
                let expected_patterns = route_patterns.get(&route.route_id).map(|s| s.len() as u32).unwrap_or(0);
                prop_assert_eq!(route.patterns, expected_patterns, "route {} patterns", route.route_id);
                prop_assert!(
                    close(route.revenue_hours, route_runtime[&route.route_id] as f64 / 3600.0, 1e-9),
                    "route {} revenue hours", route.route_id
                );
                // Typical headway: the busiest stop (most departures,
                // lowest stop id on a tie) and the upper-median gap
                // between its sorted departures — None below four
                // departures (QAF-55: a median over fewer than three
                // gaps is a headway nobody runs).
                let mut busiest: Option<(&String, usize)> = None;
                for ((r, stop), times) in &route_departures {
                    if r == &route.route_id && busiest.map_or(true, |(_, n)| times.len() > n) {
                        busiest = Some((stop, times.len()));
                    }
                }
                prop_assert_eq!(route.headway_stop.as_ref(), busiest.map(|(s, _)| s), "route {} headway stop", route.route_id);
                let typical = busiest.and_then(|(stop, _)| {
                    let mut times = route_departures[&(route.route_id.clone(), stop.clone())].clone();
                    times.sort();
                    if times.len() < 4 {
                        return None;
                    }
                    let mut gaps: Vec<u32> = times.windows(2).map(|w| w[1] - w[0]).collect();
                    gaps.sort();
                    gaps.get(gaps.len() / 2).copied()
                });
                prop_assert_eq!(route.typical_headway_secs, typical, "route {} headway", route.route_id);
            }

            // Per-stop: departures, route count, first/last, headway.
            for stop in &stats.stops {
                let Some(o) = stop_oracles.get(&stop.stop_id) else {
                    prop_assert_eq!(stop.departures, 0, "inactive stop {} reports departures", stop.stop_id);
                    continue;
                };
                prop_assert_eq!(stop.departures, o.departures, "stop {} departures", stop.stop_id);
                prop_assert_eq!(stop.routes, o.routes.len() as u32, "stop {} routes", stop.stop_id);
                prop_assert_eq!(stop.first_arrival_secs, o.first, "stop {} first", stop.stop_id);
                prop_assert_eq!(stop.last_arrival_secs, o.last, "stop {} last", stop.stop_id);
                let typical = if o.departures >= 2 {
                    Some((o.last.unwrap() - o.first.unwrap()) / (o.departures - 1))
                } else {
                    None
                };
                prop_assert_eq!(stop.typical_headway_secs, typical, "stop {} headway", stop.stop_id);
            }
        }
    }

    /// Coverage: monotone in radius, bounded by the layer, and the
    /// served-stop counts do not depend on the radius.
    #[test]
    fn coverage_is_monotone_and_bounded(
        (net, points) in spec_and_layer(),
    ) {
        let layer = transit_analysis::parse_pop_layer(&build_layer(&net, &points), Some("pop"))
            .expect("generated layer parses");
        let date = EPOCH + Duration::days(0); // a weekday: service 0 runs

        let c50 = transit_analysis::coverage(&net, date, &layer, 50.0, 3600, 2, None);
        let c200 = transit_analysis::coverage(&net, date, &layer, 200.0, 3600, 2, None);
        let c1000 = transit_analysis::coverage(&net, date, &layer, 1000.0, 3600, 2, None);

        prop_assert!(c200.served_pop >= c50.served_pop - 1e-9, "served pop not monotone 50->200");
        prop_assert!(c1000.served_pop >= c200.served_pop - 1e-9, "served pop not monotone 200->1000");
        prop_assert!(c200.frequent_pop >= c50.frequent_pop - 1e-9, "frequent pop not monotone 50->200");
        prop_assert!(c1000.frequent_pop >= c200.frequent_pop - 1e-9, "frequent pop not monotone 200->1000");

        for c in [&c50, &c200, &c1000] {
            prop_assert!(c.total_pop >= 0.0);
            prop_assert!(c.served_pop >= 0.0 && c.served_pop <= c.total_pop + 1e-9);
            prop_assert!(c.frequent_pop >= 0.0 && c.frequent_pop <= c.served_pop + 1e-9);
            prop_assert!(c.served_pct >= 0.0 && c.served_pct <= 100.0);
            prop_assert!(c.frequent_pct >= 0.0 && c.frequent_pct <= c.served_pct + 1e-9);
        }
        prop_assert_eq!(c50.served_stops, c1000.served_stops, "served stops must not depend on radius");
        prop_assert_eq!(c50.frequent_stops, c1000.frequent_stops, "frequent stops must not depend on radius");
    }

    /// Agreement: what type_coverage reports per service type equals
    /// what coverage reports on that type's representative day.
    #[test]
    fn type_coverage_agrees_with_coverage(
        (net, points) in spec_and_layer(),
    ) {
        let layer = transit_analysis::parse_pop_layer(&build_layer(&net, &points), Some("pop"))
            .expect("generated layer parses");
        let per_type = transit_analysis::type_coverage(&net, &layer, 500.0, 3600, 2, None);
        for entry in &per_type {
            let date = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d")
                .expect("representative dates are ISO");
            let whole = transit_analysis::coverage(&net, date, &layer, 500.0, 3600, 2, None);
            prop_assert!(close(entry.served_pct, whole.served_pct, 1e-9),
                "served_pct disagrees for {}", entry.service_type);
            prop_assert!(close(entry.frequent_pct, whole.frequent_pct, 1e-9),
                "frequent_pct disagrees for {}", entry.service_type);
            prop_assert_eq!(entry.served_stops, whole.served_stops, "served stops disagree for {}", entry.service_type);
            prop_assert_eq!(entry.frequent_stops, whole.frequent_stops, "frequent stops disagree for {}", entry.service_type);
        }
    }

    /// QAF-2: network_stats is date-honest — the service_types
    /// field names exactly the types whose calendar RUNS on the
    /// queried date (it used to answer every type on any date, so
    /// two different days read identically). The oracle walks each
    /// rule by hand: ISO weekday, range check, exception scan — no
    /// shared code with the model.
    #[test]
    fn service_types_match_the_date(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&gen_feed(&trips)).expect("generated feeds parse");
        let net = transit_codec::gtfs_to_network(&gtfs).network;
        // A weekday (Mon) and a weekend day (Sat) of 2026.
        for date in [
            NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
        ] {
            let stats = transit_analysis::network_stats(&net, date);
            let mut expected: BTreeSet<&str> = Default::default();
            for (id, st) in &net.service_types {
                let iso = date.format("%u").to_string();
                let flagged = match iso.as_str() {
                    "1" => st.rule.days.mon,
                    "2" => st.rule.days.tue,
                    "3" => st.rule.days.wed,
                    "4" => st.rule.days.thu,
                    "5" => st.rule.days.fri,
                    "6" => st.rule.days.sat,
                    _ => st.rule.days.sun,
                };
                let in_range = st.rule.ranges.iter().any(|r| date >= r.start && date <= r.end);
                let exception = st.rule.exceptions.iter().find(|e| e.date == date);
                let runs = match exception {
                    Some(e) => matches!(e.state, transit_network::DayState::Added),
                    None => flagged && in_range,
                };
                if runs {
                    expected.insert(id.as_str());
                }
            }
            let got: BTreeSet<&str> =
                stats.service_types.iter().map(|s| s.service_type.as_str()).collect();
            prop_assert_eq!(
                got,
                expected,
                "the service types named on {} must be the ones that run that day",
                date
            );
        }
    }
}
