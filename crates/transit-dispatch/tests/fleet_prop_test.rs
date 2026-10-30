//! The fleet laws, model-native (QAF-3): the day's blocks folded
//! into ONE answer — peak concurrent vehicles and when, plus the
//! platform / revenue / layover hour bases. The oracle recomputes
//! every number from the materialized day with independent code:
//! per-block spans and runtimes, and the peak by brute force over
//! every boundary time (half-open spans — a bus arriving as
//! another departs is one vehicle, not two) — and each running
//! block's outline (QAF-15): its span, trip count, and longest
//! internal gap, where a block might be cut.
//!
//! The generator authors blocks directly: one pattern, a set of
//! starts partitioned into consecutive blocks (strictly increasing
//! trips — the model's invariant), with a trailing group of trips
//! left unassigned (the document's warning, not vehicles).

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_dispatch::fleet_summary;
use transit_network::{
    materialize, Block, CalendarRule, DateRange, Departs, Network, Pattern, PatternTrips, PatternStop, Route,
    ServiceType, Stop, StopOffset, TimingProfile, Trip, TripRef, WeekdaySet,
};

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 5).unwrap() // a Monday
}

fn build_network(starts: Vec<u32>, block_sizes: Vec<u8>, unassigned_tail: u8) -> Network {
    let mut stops = std::collections::BTreeMap::new();
    stops.insert("A".to_string(), Stop { name: "Main & 1st".into(), lat: 45.5, lon: -122.5, parent: None, transfer: false, ..Default::default() });
    stops.insert("B".to_string(), Stop { name: "Main & 2nd".into(), lat: 45.6, lon: -122.6, parent: None, transfer: false, ..Default::default() });
    let mut routes = std::collections::BTreeMap::new();
    routes.insert("10".to_string(), Route { name: "10".into(), mode: transit_network::Mode::Bus, color: None, directions: Default::default(), ..Default::default() });
    let mut patterns = std::collections::BTreeMap::new();
    patterns.insert(
        "p0".to_string(),
        Pattern {
            direction: None,
            route: "10".into(),
            name: "route 10".into(),
            alignment: None,
            stops: vec![
                PatternStop { stop: "A".into(), timepoint: true, distance_m: 0.0, ..Default::default() },
                PatternStop { stop: "B".into(), timepoint: true, distance_m: 1000.0, ..Default::default() },
            ],
            timing: vec![TimingProfile { start_secs: 0, end_secs: 172_800, at_timepoints: vec![0, 600] }],
        },
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
    let mut pattern_trips = std::collections::BTreeMap::new();
    pattern_trips.insert(
        "s0".to_string(),
        PatternTrips {
            pattern: "p0".into(),
            service_type: "WK".into(),
            trips: starts
                .iter()
                .map(|&s| Trip {
                    id: format!("s0@{s}"),
                    departs: Departs::At { start_secs: s },
                    times: [0, 600].map(|t| StopOffset { arrival_secs: Some(t), departure_secs: Some(t) }).to_vec(),
                    ..Default::default()
                })
                .collect(),
        },
    );
    // Partition the (sorted, unique) starts into consecutive blocks;
    // the trailing `unassigned_tail` starts belong to no block.
    let mut blocks = std::collections::BTreeMap::new();
    let mut trips = starts;
    let total = trips.len();
    let assigned = total.saturating_sub(unassigned_tail as usize);
    trips.truncate(assigned);
    let mut index = 0usize;
    for (n, &size) in block_sizes.iter().enumerate() {
        if index >= trips.len() {
            break;
        }
        let end = (index + size as usize).min(trips.len());
        blocks.insert(
            format!("b{n}"),
            Block { trips: trips[index..end].iter().map(|&s| TripRef { trip: format!("s0@{s}") }).collect(), layovers: vec![] },
        );
        index = end;
    }
    Network {
        name: "fleet law net".into(),
        plan: Default::default(),
        anchor: Default::default(),
        stops,
        routes,
        patterns,
        alignments: Default::default(),
        service_types,
        pattern_trips,
        blocks,
        ..Default::default()
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The fleet oracle: every number `fleet_summary` reports is
    /// recomputed from the materialized day — per-block spans and
    /// runtimes by hand, the peak by brute force over every block
    /// boundary. Unassigned trips are not vehicles; an empty day
    /// (a Saturday nothing runs) answers zero fleet.
    #[test]
    fn fleet_summary_matches_the_span_oracle(
        slots in proptest::collection::vec(0u16..96, 0..=10),
        block_sizes in proptest::collection::vec(1u8..=3, 0..=6),
        unassigned_tail in 0u8..3,
    ) {
        // Distinct, sorted starts: one vehicle per block runs them in
        // turn.
        let starts: Vec<u32> = {
            let mut s: Vec<u32> = slots.into_iter().map(|x| x as u32 * 600).collect();
            s.sort_unstable();
            s.dedup();
            s
        };
        let net = build_network(starts.clone(), block_sizes, unassigned_tail);
        let day = materialize(&net, day()).expect("materialize");

        // Oracle: per running block, span and runtime.
        let mut spans: Vec<(u32, u32)> = Vec::new();
        let mut outlines: Vec<(String, u32, u32, u32, Option<(u32, u32)>)> = Vec::new();
        let mut platform = 0u64;
        let mut revenue = 0u64;
        for block in net.blocks.values() {
            let running: Vec<&transit_network::MaterializedTrip> = day
                .iter()
                .filter(|t| {
                    block.trips.iter().any(|r| r.trip == t.trip)
                })
                .collect();
            if running.is_empty() {
                continue;
            }
            let first = running.iter().map(|t| t.start_secs).min().unwrap();
            let last = running
                .iter()
                .map(|t| t.stop_times.last().map(|st| st.arrival_secs.max(st.departure_secs)).unwrap_or(t.start_secs))
                .max()
                .unwrap();
            spans.push((first, last));
            platform += (last - first) as u64;
            // The longest wait between one trip's end and the next's
            // start, in start order (the first of equal ones).
            let mut ordered: Vec<(u32, u32)> = running
                .iter()
                .map(|t| (t.start_secs, t.stop_times.last().map(|st| st.arrival_secs.max(st.departure_secs)).unwrap_or(t.start_secs)))
                .collect();
            ordered.sort_unstable();
            let mut longest: Option<(u32, u32)> = None;
            for pair in ordered.windows(2) {
                let gap = pair[1].0.saturating_sub(pair[0].1);
                if longest.is_none_or(|(g, _)| gap > g) {
                    longest = Some((gap, pair[0].1));
                }
            }
            let id = net.blocks.iter().find(|(_, b)| std::ptr::eq(*b, block)).unwrap().0.clone();
            outlines.push((id, running.len() as u32, first, last, longest));
            for t in &running {
                let end = t.stop_times.last().map(|st| st.arrival_secs.max(st.departure_secs)).unwrap_or(t.start_secs);
                revenue += (end - t.start_secs) as u64;
            }
        }
        // The peak by brute force: every boundary time, half-open.
        let mut peak = 0u32;
        let mut peak_at = None;
        for &(start, end) in &spans {
            for t in [start, end] {
                let concurrent = spans.iter().filter(|&&(s, e)| s <= t && t < e).count() as u32;
                if concurrent > peak {
                    peak = concurrent;
                    peak_at = Some(t);
                }
            }
        }

        let summary = fleet_summary(&net, &day);
        prop_assert_eq!(summary.blocks, spans.len() as u32, "running blocks");
        prop_assert_eq!(summary.peak_vehicles, peak, "peak concurrent vehicles");
        prop_assert_eq!(summary.peak_at_secs, peak_at, "the first time the peak is reached");
        prop_assert!((summary.platform_hours - platform as f64 / 3600.0).abs() < 1e-9, "platform hours");
        prop_assert!((summary.revenue_hours - revenue as f64 / 3600.0).abs() < 1e-9, "revenue hours");
        prop_assert!((summary.layover_hours - (platform - revenue) as f64 / 3600.0).abs() < 1e-9, "layover hours");
        prop_assert!(summary.peak_vehicles <= summary.blocks, "the peak never exceeds the fleet");
        prop_assert!(summary.layover_hours >= -1e-9, "platform is at least revenue");
        let got: Vec<(String, u32, u32, u32, Option<(u32, u32)>)> = summary
            .spans
            .iter()
            .map(|b| {
                assert_eq!(b.span_secs, b.last_end_secs - b.first_start_secs, "{}: the span is end − start", b.block_id);
                (b.block_id.clone(), b.trips, b.first_start_secs, b.last_end_secs, b.longest_gap_secs.zip(b.longest_gap_at_secs))
            })
            .collect();
        prop_assert_eq!(got, outlines, "each running block's outline, in block order");

        // An empty day answers zero fleet, never an error.
        let saturday = materialize(&net, NaiveDate::from_ymd_opt(2026, 1, 10).unwrap()).expect("materialize");
        let quiet = fleet_summary(&net, &saturday);
        prop_assert_eq!((quiet.blocks, quiet.peak_vehicles), (0, 0));
        prop_assert_eq!(quiet.peak_at_secs, None);
        prop_assert_eq!(quiet.platform_hours, 0.0);
        prop_assert!(quiet.spans.is_empty());
    }
}
