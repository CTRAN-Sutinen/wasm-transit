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
//! * **Gap oracle** — `stop_gaps` (the stop's day folded into
//!   headway gaps) is recomputed from the same materialized trips:
//!   departures, distinct-time spine, max gap and the departure it
//!   follows, per-hour departures and the max/median gap starting
//!   in each hour.
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
        0u8..4,
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
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n0,1,1,1,1,1,0,0,20260101,20261231\n1,0,0,0,0,0,1,1,20260101,20261231\n2,1,0,1,0,0,0,0,20260101,20261231\n3,1,1,1,1,1,0,0,20260301,20260601\n",
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
            let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("parse");
            let net = transit_codec::import(&feed).network;
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
        let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let net = transit_codec::import(&feed).network;

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
            // QAF-21: pattern meters per route and total — the
            // revenue-miles oracle, straight off the document's own
            // stop distances.
            let mut route_meters: BTreeMap<String, f64> = BTreeMap::new();
            let mut meters = 0f64;
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
                let pattern = &net.patterns[&trip.pattern];
                let dist = pattern.stops.last().unwrap().distance_m - pattern.stops.first().unwrap().distance_m;
                meters += dist;
                *route_meters.entry(route.clone()).or_default() += dist;
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
            prop_assert!(
                close(stats.revenue_miles, meters / 1609.344, 1e-9),
                "revenue miles on {:?}", date);

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
                prop_assert!(
                    close(route.revenue_miles, route_meters.get(&route.route_id).copied().unwrap_or(0.0) / 1609.344, 1e-9),
                    "route {} revenue miles", route.route_id
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
    ) {        let layer = transit_analysis::parse_pop_layer(&build_layer(&net, &points), Some("pop"))
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
        prop_assert!(!per_type.is_empty() || net.service_types.is_empty(), "every type answers");
        for entry in &per_type {
            let date = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d")
                .expect("representative dates are ISO");
            // Several service ids can run on the same service day
            // (the 164 defect's lesson: a weekday is 1-WKDY AND its
            // overlays), so the per-type answer cannot be compared to
            // the WHOLE-day coverage. The oracle scopes the document
            // to the type's own schedules and runs the same coverage
            // over it — the design axis, judged alone.
            let mut scoped = net.clone();
            scoped.pattern_trips
                .retain(|_, s| s.service_type == entry.service_type);
            let own = transit_analysis::coverage(&scoped, date, &layer, 500.0, 3600, 2, None);
            prop_assert!(close(entry.served_pct, own.served_pct, 1e-9),
                "served_pct disagrees for {}", entry.service_type);
            prop_assert!(close(entry.frequent_pct, own.frequent_pct, 1e-9),
                "frequent_pct disagrees for {}", entry.service_type);
            prop_assert_eq!(entry.served_stops, own.served_stops, "served stops disagree for {}", entry.service_type);
            prop_assert_eq!(entry.frequent_stops, own.frequent_stops, "frequent stops disagree for {}", entry.service_type);
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
        let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("generated feeds parse");
        let net = transit_codec::import(&feed).network;
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

    /// QAF-25: the stop's day as gaps — `stop_gaps` matches an
    /// independent fold of the same materialized departures, on
    /// every stop of the generated day and on an unknown stop.
    #[test]
    fn stop_gaps_match_the_departure_oracle(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let net = transit_codec::import(&feed).network;
        let date = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(); // a Monday

        let materialized = transit_network::materialize(&net, date).expect("materialize");
        // The oracle's raw departures per stop (every visit but the
        // trip's last — final stops are arrivals).
        let mut raw: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for trip in &materialized {
            for st in trip.stop_times.iter().rev().skip(1) {
                raw.entry(st.stop.clone()).or_default().push(st.departure_secs);
            }
        }

        for stop_id in ["s0", "s1", "s2", "s3", "s4", "s5", "nope"] {
            let gaps = transit_analysis::stop_gaps(&net, date, stop_id);
            let departures = raw.get(stop_id);
            if departures.is_none() {
                prop_assert_eq!(gaps.departures, 0, "{} is served by nothing", stop_id);
                prop_assert_eq!(gaps.first_departure_secs, None);
                prop_assert_eq!(gaps.last_departure_secs, None);
                prop_assert_eq!(gaps.max_gap_secs, None);
                prop_assert_eq!(gaps.max_gap_after_secs, None);
                prop_assert!(gaps.hourly.is_empty(), "{} has no hour rows", stop_id);
                continue;
            }
            let departures = departures.unwrap();
            prop_assert_eq!(gaps.departures, departures.len() as u32, "{} departures", stop_id);
            prop_assert_eq!(gaps.name, net.stops[stop_id].name.clone(), "{} name", stop_id);

            // The distinct-time spine, sorted.
            let spine: Vec<u32> = departures.iter().copied().collect::<BTreeSet<_>>().into_iter().collect();
            prop_assert_eq!(gaps.first_departure_secs, spine.first().copied(), "{} first", stop_id);
            prop_assert_eq!(gaps.last_departure_secs, spine.last().copied(), "{} last", stop_id);

            // Max gap and the departure it follows (None below two
            // distinct departures).
            let mut max_gap = None;
            let mut max_after = None;
            for w in spine.windows(2) {
                if max_gap.map_or(true, |m| w[1] - w[0] > m) {
                    max_gap = Some(w[1] - w[0]);
                    max_after = Some(w[0]);
                }
            }
            prop_assert_eq!(gaps.max_gap_secs, max_gap, "{} max gap", stop_id);
            prop_assert_eq!(gaps.max_gap_after_secs, max_after, "{} max gap after", stop_id);

            // Per hour: departures in the hour, and the max / median
            // gap STARTING in the hour (median None below four gaps).
            let mut oracle_hours: BTreeMap<u32, (u32, Vec<u32>)> = BTreeMap::new();
            for &d in departures {
                oracle_hours.entry(d / 3600).or_default().0 += 1;
            }
            for w in spine.windows(2) {
                oracle_hours.entry(w[0] / 3600).or_default().1.push(w[1] - w[0]);
            }
            prop_assert_eq!(gaps.hourly.len(), oracle_hours.len(), "{} hour rows", stop_id);
            for row in &gaps.hourly {
                let (dep, mut hour_gaps) = match oracle_hours.get_mut(&row.hour) {
                    Some((d, g)) => (*d, std::mem::take(g)),
                    None => (0, Vec::new()),
                };
                hour_gaps.sort();
                prop_assert_eq!(row.departures, dep, "{} hour {} departures", stop_id, row.hour);
                prop_assert_eq!(row.max_gap_secs, hour_gaps.last().copied(), "{} hour {} max", stop_id, row.hour);
                let median = (hour_gaps.len() >= 4).then(|| hour_gaps[hour_gaps.len() / 2]);
                prop_assert_eq!(row.median_gap_secs, median, "{} hour {} median", stop_id, row.hour);
            }
        }
    }

    /// QAF-4: the day's hourly profile — `hourly_trips` matches an
    /// independent fold of the same materialized trips, bucketed by
    /// start hour: per-hour totals, per-type counts, only non-empty
    /// hours, and the day's trip count conserved across the rows.
    #[test]
    fn hourly_trips_match_the_start_hour_oracle(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let net = transit_codec::import(&feed).network;

        for date in [
            NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(), // a Monday
            NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(), // a Saturday
        ] {
            let profile = transit_analysis::hourly_trips(&net, date);
            let materialized = transit_network::materialize(&net, date).expect("materialize");

            // Oracle: hour → type → count, straight off the trips.
            let mut oracle: BTreeMap<u32, BTreeMap<String, u32>> = BTreeMap::new();
            for trip in &materialized {
                let ty = net.pattern_trips[&trip.pattern_trips].service_type.clone();
                *oracle.entry(trip.start_secs / 3600).or_default().entry(ty).or_default() += 1;
            }
            prop_assert_eq!(profile.len(), oracle.len(), "only non-empty hours on {:?}", date);
            let mut row_total = 0u32;
            for row in &profile {
                let types = oracle.get(&row.hour).expect("no phantom hours");
                prop_assert_eq!(
                    row.trips,
                    types.values().sum::<u32>(),
                    "hour {} total on {:?}", row.hour, date
                );
                prop_assert_eq!(row.service_types.len(), types.len(), "hour {} type rows", row.hour);
                for ty in &row.service_types {
                    prop_assert_eq!(
                        ty.trips,
                        types.get(&ty.service_type).copied().unwrap_or(0),
                        "hour {} type {} on {:?}", row.hour, ty.service_type, date
                    );
                }
                row_total += row.trips;
            }
            prop_assert_eq!(row_total, materialized.len() as u32, "trips conserved across hours on {:?}", date);
            // Sorted by hour.
            prop_assert!(profile.windows(2).all(|w| w[0].hour < w[1].hour));
        }
    }

    /// QAF-16 (+ the 164 defect): the schedule texture — headway
    /// uniformity and clockface alignment per PATTERN, keyed by the
    /// SERVICE TYPE on its representative day (the planner's unit —
    /// a planner designs "weekday service", not a calendar date),
    /// with the day's starts POOLED across every service id running
    /// the pattern that day. Several service ids share one service
    /// day and one route (route 164's weekday trips split across
    /// 1-WKDY and its disruption-era overlays), and keying by
    /// service id would slice the corridor into thin, wrong
    /// headways. The oracle recomputes the pooling independently:
    /// raw trip counts, a distinct-start spine for gaps and grid,
    /// the feeding schedule count, and the totals.
    #[test]
    fn schedule_texture_matches_the_starts_oracle(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let net = transit_codec::import(&feed).network;

        for type_id in ["0", "1", "2", "3"] {
            let texture = transit_analysis::schedule_texture(&net, type_id);
            // The representative day: the first 2026 date the rule
            // runs, by the hand-rolled rule walk.
            let mut rep = None;
            let Some(st) = net.service_types.get(type_id) else {
                // A generated feed can leave a service unused — the
                // import creates no such type, and the op must agree.
                prop_assert!(transit_analysis::schedule_texture(&net, type_id).is_none());
                continue;
            };
            for day in 0..365i64 {
                let date = EPOCH + Duration::days(day);
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
                    rep = Some(date);
                    break;
                }
            }
            match (texture.clone(), rep) {
                (None, None) => continue,
                (Some(t), Some(date)) => assert_eq!(t.date, date.format("%Y-%m-%d").to_string()),
                (t, r) => panic!("texture {:?} vs representative {:?}", t, r),
            }
            let texture = texture.unwrap();

            // Oracle: the service GROUP — the base plus every type
            // whose running days are a subset of the base's
            // (computed by an independent day walk) — and the
            // group's POOLED starts per pattern.
            let runs_day = |sid: &str, date: chrono::NaiveDate| {
                net.service_types.get(sid).is_some_and(|st| st.rule.runs_on(date))
            };
            let day_set = |sid: &str| -> BTreeSet<chrono::NaiveDate> {
                let st = &net.service_types[sid];
                let mut days: BTreeSet<chrono::NaiveDate> = BTreeSet::new();
                for r in &st.rule.ranges {
                    let mut d = r.start;
                    while d <= r.end {
                        if st.rule.runs_on(d) {
                            days.insert(d);
                        }
                        d += Duration::days(1);
                    }
                }
                days.retain(|d| runs_day(sid, *d));
                days
            };
            let base_days = day_set(type_id);
            let group: BTreeSet<&String> = net
                .service_types
                .keys()
                .filter(|other| {
                    net.service_types[other.as_str()].rule.days == net.service_types[type_id].rule.days
                        && day_set(other).iter().all(|d| base_days.contains(d))
                })
                .collect();
            prop_assert!(group.iter().any(|s| s.as_str() == type_id), "the base is in its own group");
            let mut oracle: BTreeMap<String, (u32, Vec<u32>)> = BTreeMap::new();
            for entry in net.pattern_trips.values() {
                if !group.contains(&entry.service_type) {
                    continue;
                }
                let slot = oracle.entry(entry.pattern.clone()).or_insert((0, Vec::new()));
                slot.0 += 1;
                slot.1.extend(entry.trips.iter().flat_map(|t| t.departs.instances()));
            }
            prop_assert_eq!(texture.pattern_trips.len(), oracle.len(), "{} pattern rows", type_id);
            let mut sum_trips = 0u32;
            let mut sum_grid = 0u32;
            for row in &texture.pattern_trips {
                let (schedules, raw) = &oracle[&row.pattern];
                prop_assert_eq!(row.schedules, *schedules, "{} feeding schedules", row.pattern);
                prop_assert_eq!(row.trips, raw.len() as u32, "{} pooled trips", row.pattern);
                let mut spine = raw.clone();
                spine.sort_unstable();
                spine.dedup();
                let gaps: Vec<u32> = spine.windows(2).map(|w| w[1] - w[0]).collect();
                prop_assert_eq!(row.min_gap_secs, gaps.iter().copied().min(), "{} min", row.pattern);
                prop_assert_eq!(row.max_gap_secs, gaps.iter().copied().max(), "{} max", row.pattern);
                let median = (spine.len() >= 4).then(|| {
                    let mut sorted = gaps.clone();
                    sorted.sort_unstable();
                    sorted[sorted.len() / 2]
                });
                prop_assert_eq!(row.median_gap_secs, median, "{} median", row.pattern);
                let grid = spine.iter().filter(|&&s| s % 900 == 0).count() as u32;
                prop_assert_eq!(row.on_clockface, grid, "{} grid", row.pattern);
                sum_trips += row.trips;
                sum_grid += row.on_clockface;
            }
            prop_assert_eq!(texture.trips, sum_trips, "{} trips total", type_id);
            prop_assert_eq!(texture.on_clockface, sum_grid, "{} grid total", type_id);
            prop_assert!(texture.on_clockface <= texture.trips);
        }

        // Unknown types answer None, never an error (QAF-19).
        prop_assert!(transit_analysis::schedule_texture(&net, "nope").is_none());
    }

    /// QAF-22: the corridor read — `route_overlap` matches an
    /// independent fold of the same materialized day: every pair of
    /// routes that shares a stop, the shared-stop and shared-segment
    /// counts, and the shared stop where the pair's departures stack
    /// highest (ties by stop id, lowest wins).
    #[test]
    fn route_overlap_matches_the_pair_oracle(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let net = transit_codec::import(&feed).network;
        let date = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(); // a Monday

        let overlaps = transit_analysis::route_overlap(&net, date);
        let materialized = transit_network::materialize(&net, date).expect("materialize");

        // Oracle: per route, served stops, ordered segments, and
        // departures per stop — then every pair, by hand.
        let mut stops: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut segs: BTreeMap<String, BTreeSet<(String, String)>> = BTreeMap::new();
        let mut deps: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
        for trip in &materialized {
            let route = net.patterns[&trip.pattern].route.clone();
            stops.entry(route.clone()).or_default()
                .extend(trip.stop_times.iter().map(|st| st.stop.clone()));
            let pattern_segs: Vec<(String, String)> = net.patterns[&trip.pattern].stops.windows(2)
                .map(|w| (w[0].stop.clone(), w[1].stop.clone())).collect();
            segs.entry(route.clone()).or_default().extend(pattern_segs);
            for st in trip.stop_times.iter().rev().skip(1) {
                *deps.entry(route.clone()).or_default().entry(st.stop.clone()).or_default() += 1;
            }
        }

        let mut expected: Vec<(String, String, u32, u32, String, u32)> = Vec::new();
        let ids: Vec<&String> = stops.keys().collect();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let shared: BTreeSet<&String> =
                    stops[ids[i]].intersection(&stops[ids[j]]).collect();
                if shared.is_empty() {
                    continue;
                }
                let shared_segments = segs[ids[i]].intersection(&segs[ids[j]]).count() as u32;
                let mut best: Option<(String, u32)> = None;
                for stop in &shared {
                    let combined = deps[ids[i]].get(*stop).copied().unwrap_or(0)
                        + deps[ids[j]].get(*stop).copied().unwrap_or(0);
                    if best.as_ref().map_or(true, |(b, c)| combined > *c || (combined == *c && *stop < b)) {
                        best = Some((stop.to_string(), combined));
                    }
                }
                let (stack_stop, stack_departures) = best.expect("shared stops exist");
                expected.push((ids[i].clone(), ids[j].clone(), shared.len() as u32, shared_segments, stack_stop, stack_departures));
            }
        }
        prop_assert_eq!(overlaps.len(), expected.len(), "pair rows");
        for (row, exp) in overlaps.iter().zip(&expected) {
            prop_assert_eq!(&row.a, &exp.0);
            prop_assert_eq!(&row.b, &exp.1);
            prop_assert_eq!(row.shared_stops, exp.2, "{} + {} stops", row.a, row.b);
            prop_assert_eq!(row.shared_segments, exp.3, "{} + {} segments", row.a, row.b);
            prop_assert_eq!(row.stack_stop.as_deref(), Some(exp.4.as_str()), "{} + {} stack stop", row.a, row.b);
            prop_assert_eq!(row.stack_departures, exp.5, "{} + {} stack", row.a, row.b);
        }
    }

    /// QAF-27: the hub's timed-transfer view — every arrival→
    /// departure pair across routes inside the caller's connection
    /// window (defaulting to the 60–900 s convention), pooled over
    /// the walking group. The oracle recomputes the pairs from the
    /// materialized day and checks the sort, the window bounds, and
    /// that no connection pairs a route with itself.
    #[test]
    fn timed_transfers_match_the_window_oracle(
        trips in proptest::collection::vec(gen_trip(), 0..=14),
        min_secs in 0u32..1200,
        max_extra in 0u32..1500,
        use_default in any::<bool>(),
    ) {
        let window = if use_default {
            None
        } else {
            Some(transit_analysis::TransferWindow { min_secs, max_secs: min_secs + max_extra })
        };
        let (lo, hi) = match window {
            Some(w) => (w.min_secs, w.max_secs),
            None => (60, 900),
        };
        let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let net = transit_codec::import(&feed).network;
        let date = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(); // a Monday
        let materialized = transit_network::materialize(&net, date).expect("materialize");

        // Every stop of the document is asked — the hub may be any
        // of them, and a plain stop pools only itself.
        for stop_id in net.stops.keys() {
            let transfers = transit_analysis::timed_transfers(&net, date, stop_id, window);

            // Oracle: the walking group by hand (stop, siblings
            // sharing a parent, children, the parent), then the
            // arrival/departure lists, then the pairs.
            let mut group: BTreeSet<&String> = BTreeSet::new();
            group.insert(stop_id);
            let parent = net.stops[stop_id].parent.clone();
            for (id, s) in &net.stops {
                if id == stop_id
                    || (parent.is_some() && s.parent == parent)
                    || s.parent.as_deref() == Some(stop_id.as_str())
                    || Some(id.as_str()) == parent.as_deref()
                {
                    group.insert(id);
                }
            }
            let mut arrivals: Vec<(String, String, u32)> = Vec::new();
            let mut departures: Vec<(String, String, u32)> = Vec::new();
            for trip in &materialized {
                let route = net.patterns[&trip.pattern].route.clone();
                for (i, st) in trip.stop_times.iter().enumerate() {
                    if !group.contains(&st.stop) {
                        continue;
                    }
                    arrivals.push((route.clone(), st.stop.clone(), st.arrival_secs));
                    if i + 1 < trip.stop_times.len() {
                        departures.push((route.clone(), st.stop.clone(), st.departure_secs));
                    }
                }
            }
            let mut expected: Vec<(u32, String, String, u32)> = Vec::new();
            for (fr, _, at) in &arrivals {
                for (tr, _, dt) in &departures {
                    let wait = dt.saturating_sub(*at);
                    if fr != tr && lo <= wait && wait <= hi {
                        expected.push((*at, fr.clone(), tr.clone(), *dt));
                    }
                }
            }
            expected.sort();
            prop_assert_eq!(transfers.len(), expected.len(), "{} rows at {}", stop_id, stop_id);
            for (row, exp) in transfers.iter().zip(&expected) {
                prop_assert_eq!(row.arrival_secs, exp.0, "arrival at {}", stop_id);
                prop_assert_eq!(&row.from_route, &exp.1);
                prop_assert_eq!(&row.to_route, &exp.2);
                prop_assert_eq!(row.departure_secs, exp.3);
                prop_assert_eq!(row.wait_secs, row.departure_secs.saturating_sub(row.arrival_secs));
                prop_assert!(row.wait_secs >= lo && row.wait_secs <= hi, "the window bounds hold");
                prop_assert_ne!(&row.from_route, &row.to_route, "no connection pairs a route with itself");
                prop_assert!(group.contains(&row.arrival_stop) && group.contains(&row.departure_stop));
            }
            // The sort: non-decreasing by arrival, then routes.
            assert!(transfers.windows(2).all(|w| {
                (w[0].arrival_secs, &w[0].from_route, &w[0].to_route)
                    <= (w[1].arrival_secs, &w[1].from_route, &w[1].to_route)
            }));
        }

        // An unknown stop answers empty, never an error.
        prop_assert!(transit_analysis::timed_transfers(&net, date, "nope", window).is_empty());
    }

    /// QAF-23: the unserved WHERE — `unserved_points` names every
    /// layer point beyond the radius of every served stop, with the
    /// nearest served stop and its distance. The oracle recomputes
    /// the served set and the distances independently, and the
    /// unserved population plus the coverage answer's served
    /// population conserves the layer total.
    #[test]
    fn unserved_points_match_the_radius_oracle(
        (net, points) in spec_and_layer(),
        radius in 50f64..1000.0,
    ) {
        let layer = transit_analysis::parse_pop_layer(&build_layer(&net, &points), Some("pop"))
            .expect("generated layer parses");
        let date = EPOCH + Duration::days(0); // a weekday: service 0 runs

        let unserved = transit_analysis::unserved_points(&net, date, &layer, radius);

        // Oracle: the served stops (any visit on the date), then
        // every point's nearest one.
        let materialized = transit_network::materialize(&net, date).expect("materialize");
        let mut served: BTreeMap<String, (f64, f64)> = BTreeMap::new();
        for trip in &materialized {
            for st in &trip.stop_times {
                if let Some(stop) = net.stops.get(&st.stop) {
                    served.insert(st.stop.clone(), (stop.lat, stop.lon));
                }
            }
        }
        let mut expected: Vec<(f64, f64, f64, Option<String>, f64)> = Vec::new();
        for p in &layer.points {
            let nearest = served
                .iter()
                .map(|(id, (lat, lon))| {
                    // Haversine by hand — independent of the module.
                    let (p1, p2) = (p.lat.to_radians(), lat.to_radians());
                    let dl = (lon - p.lon).to_radians();
                    let dp = (lat - p.lat).to_radians();
                    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
                    let d = 2.0 * 6_371_000.0 * a.sqrt().min(1.0).asin();
                    (id.clone(), d)
                })
                .min_by(|x, y| x.1.partial_cmp(&y.1).unwrap());
            match nearest {
                Some((id, d)) if d > radius => expected.push((p.lat, p.lon, p.pop, Some(id), d)),
                Some(_) => continue,
                None => expected.push((p.lat, p.lon, p.pop, None, 0.0)),
            }
        }
        expected.sort_by(|a, b| {
            b.2.partial_cmp(&a.2).unwrap()
                .then_with(|| a.0.partial_cmp(&b.0).unwrap())
                .then_with(|| a.1.partial_cmp(&b.1).unwrap())
        });
        prop_assert_eq!(unserved.len(), expected.len(), "the unserved count matches the oracle");
        for (row, exp) in unserved.iter().zip(&expected) {
            prop_assert!((row.lat - exp.0).abs() < 1e-9, "lat");
            prop_assert!((row.lon - exp.1).abs() < 1e-9, "lon");
            prop_assert!((row.pop - exp.2).abs() < 1e-9, "pop");
            prop_assert_eq!(&row.nearest_stop, &exp.3, "the nearest served stop");
            prop_assert!((row.nearest_m - exp.4).abs() < 1.0, "nearest distance within a meter");
        }

        // Conservation with the coverage answer: the unserved
        // population is exactly what the served share leaves out.
        let c = transit_analysis::coverage(&net, date, &layer, radius, 3600, 2, None);
        let unserved_pop: f64 = unserved.iter().map(|p| p.pop).sum();
        prop_assert!(
            (c.total_pop - c.served_pop - unserved_pop).abs() < 1e-6,
            "served + unserved conserves the layer total"
        );
    }

    /// QAF-65: the service groups — the families behind the ids. A
    /// base type (no other type's days properly contain it) heads
    /// one row; every type whose calendar is a subset of the base's
     /// rides along (the overlays are really part of the base's
    /// service). The oracle computes day-sets independently, then
    /// the bases, the members, and the family's pooled numbers.
    #[test]
    fn service_groups_are_the_families(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let net = transit_codec::import(&feed).network;
        let groups = transit_analysis::service_groups(&net);

        // Oracle: day-sets per type (2026 walk over ranges +
        // Added exceptions), then bases and members by subset.
        let day_set = |sid: &str| -> BTreeSet<chrono::NaiveDate> {
            let st = &net.service_types[sid];
            let mut days: BTreeSet<chrono::NaiveDate> = BTreeSet::new();
            for r in &st.rule.ranges {
                let mut d = r.start;
                while d <= r.end {
                    if st.rule.runs_on(d) {
                        days.insert(d);
                    }
                    d += Duration::days(1);
                }
            }
            days
        };
        let ids: Vec<&String> = net.service_types.keys().collect();
        // Same WEEKLY PATTERN and a day-set subset: the flags are
        // the family's identity (a Tuesday-only service is its own
        // family however few days it runs), the subset says it
        // never runs a day the base does not.
        let same_days_pattern = |a: &str, b: &str| {
            net.service_types[a].rule.days == net.service_types[b].rule.days
        };
        let is_base = |t: &str| {
            let dt = day_set(t);
            !ids.iter().any(|u| {
                u.as_str() != t
                    && same_days_pattern(u, t)
                    && {
                        let du = day_set(u);
                        dt.iter().all(|d| du.contains(d)) && !du.iter().all(|d| dt.contains(d))
                    }
            })
        };
        let expected_bases: Vec<&String> =
            ids.iter().copied().filter(|t| is_base(t)).collect();
        prop_assert_eq!(groups.len(), expected_bases.len(), "one row per base");
        for group in &groups {
            let base_days = day_set(&group.service_type);
            let members: BTreeSet<String> = ids
                .iter()
                .filter(|m| {
                    same_days_pattern(m, &group.service_type)
                        && day_set(m).iter().all(|d| base_days.contains(d))
                })
                .map(|m| (*m).clone())
                .collect();
            prop_assert_eq!(
                group.members.iter().collect::<BTreeSet<_>>(),
                members.iter().collect::<BTreeSet<_>>(),
                "{} members", group.service_type
            );
            assert!(members.contains(&group.service_type), "the base is in its own family");
            // The family's pooled numbers, recomputed.
            let mut trips_n = 0u32;
            let mut routes: BTreeSet<String> = BTreeSet::new();
            let mut stops: BTreeSet<String> = BTreeSet::new();
            let mut runtime = 0u64;
            for (schedule_id, entry) in &net.pattern_trips {
                if !members.contains(&entry.service_type) {
                    continue;
                }
                if let Some(p) = net.patterns.get(&entry.pattern) {
                    routes.insert(p.route.clone());
                }
                for trip in transit_network::materialize_service(&net, schedule_id).expect("valid document") {
                    trips_n += 1;
                    stops.extend(trip.stop_times.iter().map(|st| st.stop.clone()));
                    if let Some(end) = trip.stop_times.last() {
                        runtime += end.arrival_secs.saturating_sub(trip.start_secs) as u64;
                    }
                }
            }
            prop_assert_eq!(group.trips, trips_n, "{} trips", group.service_type);
            prop_assert_eq!(group.active_routes, routes.len() as u32, "{} routes", group.service_type);
            prop_assert_eq!(group.active_stops, stops.len() as u32, "{} stops", group.service_type);
            prop_assert!(
                (group.revenue_hours - runtime as f64 / 3600.0).abs() < 1e-9,
                "{} revenue hours", group.service_type
            );
        }
        // A non-base type appears only as a member, never as a row.
        for t in &ids {
            if !is_base(t) {
                prop_assert!(
                    !groups.iter().any(|g| g.service_type == t.as_str()),
                    "{} is a rider, not a row", t
                );
                prop_assert!(
                    groups.iter().any(|g| g.members.contains(t)),
                    "{} rides in exactly some family", t
                );
            }
        }
    }

    /// The frequency grid's read — `route_frequency` matches an
    /// independent fold of the service GROUP's pooled trips (the
    /// family `schedule_texture` reads, no date filter): one row per
    /// route with departures, at the route's headway stop (the most
    /// departures, lowest id on a tie — the stop `service_type_detail`
    /// reads its headway at), and that stop's day by hour — every
    /// departure counted, gaps between distinct departure times
    /// belonging to the hour they start in, shortest, longest, and
    /// the median with the four-gap floor. The gaps are the trips'
    /// own: nothing is rounded or regularized.
    #[test]
    fn route_frequency_matches_the_headway_stop_oracle(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let net = transit_codec::import(&feed).network;

        for type_id in ["0", "1", "2", "3"] {
            let answer = transit_analysis::route_frequency(&net, type_id);
            let detail = transit_analysis::service_type_detail(&net, type_id);
            prop_assert_eq!(answer.is_some(), detail.is_some(), "{} answers like the detail", type_id);
            let (Some(answer), Some(detail)) = (answer, detail) else { continue };
            prop_assert_eq!(&answer.date, &detail.date);

            let group = transit_network::service_family(&net, type_id);
            let mut departures: BTreeMap<String, BTreeMap<String, Vec<u32>>> = BTreeMap::new();
            for (id, entry) in &net.pattern_trips {
                if !group.contains(&entry.service_type) {
                    continue;
                }
                let Ok(day) = transit_network::materialize_service(&net, id) else { continue };
                for trip in day {
                    let route = net.patterns[&trip.pattern].route.clone();
                    for st in &trip.stop_times[..trip.stop_times.len() - 1] {
                        departures.entry(route.clone()).or_default().entry(st.stop.clone()).or_default().push(st.departure_secs);
                    }
                }
            }
            prop_assert_eq!(
                answer.routes.iter().map(|r| r.route_id.clone()).collect::<Vec<_>>(),
                departures.keys().cloned().collect::<Vec<_>>(),
                "{} rows: the routes that depart, by id", type_id
            );
            for row in &answer.routes {
                let stops = &departures[&row.route_id];
                let most = stops.values().map(|v| v.len()).max().unwrap();
                let stop = stops.iter().find(|(_, v)| v.len() == most).map(|(s, _)| s.clone()).unwrap();
                prop_assert_eq!(row.headway_stop.as_deref(), Some(stop.as_str()), "{} headway stop", row.route_id);
                let stats = detail.routes.iter().find(|r| r.route_id == row.route_id).expect("the detail lists it");
                prop_assert_eq!(&row.headway_stop, &stats.headway_stop, "the detail's headway stop");
                // The row carries the route's numbers, the detail's own.
                prop_assert_eq!(
                    (row.stats.trips, row.stats.first_start_secs, row.stats.last_arrival_secs, row.stats.typical_headway_secs),
                    (stats.trips, stats.first_start_secs, stats.last_arrival_secs, stats.typical_headway_secs)
                );
                prop_assert!(close(row.stats.revenue_hours, stats.revenue_hours, 1e-9) && close(row.stats.revenue_miles, stats.revenue_miles, 1e-9));

                let times = &stops[&stop];
                let mut spine = times.clone();
                spine.sort_unstable();
                spine.dedup();
                let mut by_hour: BTreeMap<u32, (u32, Vec<u32>)> = BTreeMap::new();
                for t in times {
                    by_hour.entry(t / 3600).or_default().0 += 1;
                }
                for w in spine.windows(2) {
                    by_hour.entry(w[0] / 3600).or_default().1.push(w[1] - w[0]);
                }
                prop_assert_eq!(row.hours.len(), by_hour.len(), "{} hours", row.route_id);
                for (h, (hour, (count, gaps))) in row.hours.iter().zip(&by_hour) {
                    let mut gaps = gaps.clone();
                    gaps.sort_unstable();
                    prop_assert_eq!(h.hour, *hour);
                    prop_assert_eq!(h.departures, *count, "{} {}:00 departures", row.route_id, hour);
                    prop_assert_eq!(h.shortest_gap_secs, gaps.first().copied());
                    prop_assert_eq!(h.longest_gap_secs, gaps.last().copied());
                    prop_assert_eq!(h.median_gap_secs, (gaps.len() >= 4).then(|| gaps[gaps.len() / 2]));
                }
            }
        }
        prop_assert!(transit_analysis::route_frequency(&net, "nope").is_none(), "an unknown type answers None");
    }

    /// The hub's route-pair read — `hub_connections` is the fold of
    /// `timed_transfers` by (from route, to route): how many timed
    /// connections the pair makes, the shortest and longest wait,
    /// and the median wait with the four-connection floor; pairs in
    /// route order, none with no connection.
    #[test]
    fn hub_connections_fold_the_timed_transfers(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let net = transit_codec::import(&feed).network;
        let date = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(); // a Monday
        for stop_id in net.stops.keys() {
            let pairs = transit_analysis::hub_connections(&net, date, stop_id, None);
            let mut oracle: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();
            for t in transit_analysis::timed_transfers(&net, date, stop_id, None) {
                oracle.entry((t.from_route, t.to_route)).or_default().push(t.wait_secs);
            }
            prop_assert_eq!(pairs.len(), oracle.len(), "pairs at {}", stop_id);
            for (row, ((from, to), waits)) in pairs.iter().zip(&oracle) {
                let mut waits = waits.clone();
                waits.sort_unstable();
                prop_assert_eq!((&row.from_route, &row.to_route), (from, to));
                prop_assert_eq!(row.connections, waits.len() as u32);
                prop_assert_eq!(row.shortest_wait_secs, waits[0]);
                prop_assert_eq!(row.longest_wait_secs, *waits.last().unwrap());
                prop_assert_eq!(row.median_wait_secs, (waits.len() >= 4).then(|| waits[waits.len() / 2]));
            }
        }
        prop_assert!(transit_analysis::hub_connections(&net, date, "nope", None).is_empty());
    }
}

/// A route's hour reports the gaps its trips actually leave: trips at
/// :00, :10, :30 and 1:00 leave gaps of 10, 20 and 30 minutes in the
/// first hour — the shortest and longest as they are, never snapped
/// to a regular headway (the user, 2026-10-30: a target headway is
/// adjusted through the day; the variation is information).
#[test]
fn an_hours_gaps_are_the_trips_own() {
    let trip = |start_slot| GenTrip { route: 0, service: 0, block: None, start_slot, stops: vec![0, 1], leg_min: 5 };
    let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&[trip(0), trip(1), trip(3), trip(6)])).expect("parse");
    let net = transit_codec::import(&feed).network;
    let answer = transit_analysis::route_frequency(&net, "0").expect("a weekday type");
    let hours: Vec<_> = answer.routes[0]
        .hours
        .iter()
        .map(|h| (h.hour, h.departures, h.shortest_gap_secs, h.longest_gap_secs, h.median_gap_secs))
        .collect();
    assert_eq!(hours, [(0, 3, Some(600), Some(1800), None), (1, 1, None, None, None)]);
}
