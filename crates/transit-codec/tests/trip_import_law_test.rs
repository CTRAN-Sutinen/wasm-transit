//! The import's trip laws: a feed's trips ARE the network's trips —
//! the agency's ids, every scheduled time exactly, frequency trips as
//! frequency trips — and patterns are stop sequences, not timings.
//!
//! Written BEFORE the implementation, as the definition of the
//! behavior; the expected answers come from the generator's own
//! values, never from parsing or from the implementation. The feed is
//! built in memory (`transit_codec::Feed`), so no parser sits between
//! the generator and the import.
//!
//! The API these laws require, beyond `transit-network`'s trip laws
//! (`crates/transit-network/tests/trip_law_test.rs`):
//! `transit_codec::import(&Feed) -> ImportOutcome` with `.network`.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use transit_codec::feed::{
    AgencyRow, CalendarRow, FrequencyRow, RouteRow, ShapeRow, StopRow, StopTimeRow, TripRow,
};
use transit_codec::Feed;
use transit_network::{materialize_service, validation_issues, Departs, HeadwayBand, Network, TripRef};

// ===================================================================
// Generator: spec-valid feeds.
// ===================================================================

#[derive(Clone, Debug)]
struct GenTrip {
    route: u8,
    service: u8,
    direction: Option<u8>,
    shape: Option<u8>,
    /// Ordered distinct stops (2..=4 of s0..s5).
    stops: Vec<u8>,
    /// A 5-minute slot from a small pool: duplicates are the point.
    start: u32,
    /// Per stop: (leg seconds to reach it, dwell seconds, which times
    /// the row carries — 0 both, 1 arrival only, 2 departure only, 3
    /// neither — and its timepoint cell). First and last rows carry
    /// both times; a row missing a time is marked approximate.
    rows: Vec<(u32, u32, u8, Option<bool>)>,
    /// `Some` = a frequency trip: (start, end, headway, exact_times).
    every: Option<(u32, u32, u32, bool)>,
    block: Option<u8>,
}

fn gen_trip() -> impl Strategy<Value = GenTrip> {
    (2usize..=4).prop_flat_map(|n| {
        (
            (0u8..2, 0u8..2, proptest::option::of(0u8..2), proptest::option::of(0u8..2)),
            proptest::sample::subsequence((0u8..6).collect::<Vec<u8>>(), n),
            (60u32..90).prop_map(|slot| slot * 300),
            proptest::collection::vec((60u32..900, 0u32..120, 0u8..4, proptest::option::of(any::<bool>())), n),
            proptest::option::weighted(
                0.15,
                (6u32 * 3600..9 * 3600, 1u32..3, 600u32..1800, any::<bool>())
                    .prop_map(|(s, h, w, exact)| (s - s % 60, s - s % 60 + h * 3600, w - w % 60, exact)),
            ),
            proptest::option::of(0u8..2),
        )
            .prop_map(|((route, service, direction, shape), stops, start, rows, every, block)| GenTrip {
                route,
                service,
                direction,
                shape,
                stops,
                start,
                rows,
                every,
                block,
            })
    })
}

/// One row's scheduled times, as absolute seconds, and its
/// normalized exactness (an empty timepoint cell on a timed row is
/// exact).
#[derive(Clone, Debug)]
struct Row {
    stop: String,
    arrival: Option<u32>,
    departure: Option<u32>,
    timepoint: Option<bool>,
}

fn rows_of(t: &GenTrip) -> Vec<Row> {
    let last = t.rows.len() - 1;
    let mut at = t.start;
    t.rows
        .iter()
        .enumerate()
        .map(|(i, &(leg, dwell, shown, tp))| {
            if i > 0 {
                at += leg;
            }
            let arrival = at;
            at += if i == 0 || i == last { 0 } else { dwell };
            let shown = if i == 0 || i == last { 0 } else { shown };
            Row {
                stop: format!("s{}", t.stops[i]),
                arrival: (shown == 0 || shown == 1).then_some(arrival),
                departure: (shown == 0 || shown == 2).then_some(at),
                // A row missing a time must say it is approximate: an
                // exact row (timepoint 1, or empty on a timed feed)
                // carries both, by the spec.
                timepoint: if shown != 0 { Some(false) } else { tp },
            }
        })
        .collect()
}

fn hms(secs: u32) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// Blocks that respect the spec: a block's trips on one service never
/// overlap in time (frequency trips stay out of blocks).
fn assign_blocks(trips: &[GenTrip]) -> Vec<Option<String>> {
    let mut taken: BTreeMap<(u8, u8), Vec<(u32, u32)>> = BTreeMap::new();
    trips
        .iter()
        .map(|t| {
            let b = t.block.filter(|_| t.every.is_none())?;
            let rows = rows_of(t);
            let span = (t.start, rows.last().unwrap().arrival.unwrap());
            let spans = taken.entry((b, t.service)).or_default();
            if spans.iter().any(|&(s, e)| span.0 <= e && s <= span.1) {
                return None;
            }
            spans.push(span);
            Some(format!("b{b}"))
        })
        .collect()
}

fn build_feed(trips: &[GenTrip]) -> Feed {
    let s = |v: &str| Some(v.to_string());
    let mut feed = Feed::default();
    feed.agency.push(AgencyRow {
        agency_name: s("T"),
        agency_url: s("https://example.org"),
        agency_timezone: s("UTC"),
        ..Default::default()
    });
    for i in 0..6 {
        feed.stops.push(StopRow {
            stop_id: Some(format!("s{i}")),
            stop_name: Some(format!("Stop {i}")),
            stop_lat: Some(format!("45.5{i}")),
            stop_lon: Some(format!("-122.6{i}")),
            ..Default::default()
        });
    }
    for r in 0..2 {
        feed.routes.push(RouteRow {
            route_id: Some(r.to_string()),
            route_short_name: Some(r.to_string()),
            route_type: s("3"),
            ..Default::default()
        });
    }
    for sh in 0..2 {
        for k in 0..3 {
            feed.shapes.push(ShapeRow {
                shape_id: Some(format!("sh{sh}")),
                shape_pt_lat: Some(format!("45.{sh}{k}")),
                shape_pt_lon: Some(format!("-122.{k}{sh}")),
                shape_pt_sequence: Some(k.to_string()),
                ..Default::default()
            });
        }
    }
    for (id, days) in [("0", "1,1,1,1,1,0,0"), ("1", "0,0,0,0,0,1,1")] {
        let d: Vec<Option<String>> = days.split(',').map(|x| Some(x.to_string())).collect();
        feed.calendar.push(CalendarRow {
            service_id: s(id),
            monday: d[0].clone(),
            tuesday: d[1].clone(),
            wednesday: d[2].clone(),
            thursday: d[3].clone(),
            friday: d[4].clone(),
            saturday: d[5].clone(),
            sunday: d[6].clone(),
            start_date: s("20260101"),
            end_date: s("20261231"),
        });
    }
    let blocks = assign_blocks(trips);
    for (i, t) in trips.iter().enumerate() {
        let id = format!("t{i}");
        feed.trips.push(TripRow {
            route_id: Some(t.route.to_string()),
            service_id: Some(t.service.to_string()),
            trip_id: Some(id.clone()),
            direction_id: t.direction.map(|d| d.to_string()),
            shape_id: t.shape.map(|sh| format!("sh{sh}")),
            block_id: blocks[i].clone(),
            ..Default::default()
        });
        for (j, row) in rows_of(t).iter().enumerate() {
            feed.stop_times.push(StopTimeRow {
                trip_id: Some(id.clone()),
                stop_id: Some(row.stop.clone()),
                // Sequence numbers with gaps, as the spec allows.
                stop_sequence: Some((j * 10 + 1).to_string()),
                arrival_time: row.arrival.map(hms),
                departure_time: row.departure.map(hms),
                timepoint: row.timepoint.map(|b| if b { "1" } else { "0" }.to_string()),
                ..Default::default()
            });
        }
        if let Some((start, end, headway, exact)) = t.every {
            feed.frequencies.push(FrequencyRow {
                trip_id: Some(id),
                start_time: Some(hms(start)),
                end_time: Some(hms(end)),
                headway_secs: Some(headway.to_string()),
                exact_times: Some(if exact { "1" } else { "0" }.to_string()),
            });
        }
    }
    feed
}

/// Where a model trip lives: its pattern trips key and record.
fn find<'a>(net: &'a Network, id: &str) -> Option<(&'a str, &'a transit_network::PatternTrips, &'a transit_network::Trip)> {
    net.pattern_trips
        .iter()
        .find_map(|(k, pt)| pt.trips.iter().find(|t| t.id == id).map(|t| (k.as_str(), pt, t)))
}

fn trips_strategy() -> impl Strategy<Value = Vec<GenTrip>> {
    proptest::collection::vec(gen_trip(), 1..=10)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// The import of a spec-valid feed is a valid document.
    #[test]
    fn the_import_is_valid(trips in trips_strategy()) {
        let net = transit_codec::import(&build_feed(&trips)).network;
        let issues = validation_issues(&net);
        prop_assert!(issues.is_empty(), "{:?}", issues);
    }

    /// The feed's trips are the network's trips, by the agency's ids:
    /// none dropped, none merged (two vehicles on one schedule are two
    /// trips), none invented.
    #[test]
    fn the_agency_ids_are_the_trip_ids(trips in trips_strategy()) {
        let net = transit_codec::import(&build_feed(&trips)).network;
        let model: BTreeSet<String> = net.pattern_trips.values().flat_map(|pt| pt.trips.iter().map(|t| t.id.clone())).collect();
        let feed: BTreeSet<String> = (0..trips.len()).map(|i| format!("t{i}")).collect();
        prop_assert_eq!(model, feed);
    }

    /// Every scheduled time survives exactly — arrivals and departures,
    /// timepoints or not; an unscheduled row stays unscheduled; the
    /// trip runs on its feed service.
    #[test]
    fn every_scheduled_time_survives(trips in trips_strategy()) {
        let net = transit_codec::import(&build_feed(&trips)).network;
        for (i, t) in trips.iter().enumerate() {
            if t.every.is_some() {
                continue; // the frequency law below
            }
            let id = format!("t{i}");
            let (key, pt, _) = find(&net, &id).ok_or_else(|| TestCaseError::fail(format!("{id} missing")))?;
            prop_assert_eq!(&pt.service_type, &t.service.to_string(), "{}: its service", id);
            let got = materialize_service(&net, key).unwrap();
            let m = got.iter().find(|m| m.trip == id).ok_or_else(|| TestCaseError::fail(format!("{id} not materialized")))?;
            let rows = rows_of(t);
            prop_assert_eq!(m.stop_times.len(), rows.len(), "{}: its stops", id);
            for (st, row) in m.stop_times.iter().zip(&rows) {
                prop_assert_eq!(&st.stop, &row.stop);
                if let Some(a) = row.arrival {
                    prop_assert_eq!(st.arrival_secs, a, "{} at {}: the feed's arrival", id, row.stop);
                }
                if let Some(d) = row.departure {
                    prop_assert_eq!(st.departure_secs, d, "{} at {}: the feed's departure", id, row.stop);
                }
            }
            let (_, _, trip) = find(&net, &id).unwrap();
            for (o, row) in trip.times.iter().zip(&rows) {
                prop_assert_eq!(o.arrival_secs.is_some(), row.arrival.is_some(), "{}: scheduled arrivals stay scheduled", id);
                prop_assert_eq!(o.departure_secs.is_some(), row.departure.is_some(), "{}: scheduled departures stay scheduled", id);
            }
        }
    }

    /// A frequency trip imports as one: the agency's id, its windows
    /// and headways, its exactness, and its stop times as offsets from
    /// its first arrival.
    #[test]
    fn frequency_trips_stay_frequency_trips(trips in trips_strategy()) {
        let net = transit_codec::import(&build_feed(&trips)).network;
        for (i, t) in trips.iter().enumerate() {
            let Some((start, end, headway, exact)) = t.every else { continue };
            let id = format!("t{i}");
            let (_, _, trip) = find(&net, &id).ok_or_else(|| TestCaseError::fail(format!("{id} missing")))?;
            prop_assert_eq!(
                &trip.departs,
                &Departs::Every { bands: vec![HeadwayBand { start_secs: start, end_secs: end, headway_secs: headway }], exact }
            );
            for (o, row) in trip.times.iter().zip(rows_of(t)) {
                prop_assert_eq!(o.arrival_secs, row.arrival.map(|a| a - t.start));
                prop_assert_eq!(o.departure_secs, row.departure.map(|d| d - t.start));
            }
        }
    }

    /// Patterns are stop sequences: two trips share a pattern exactly
    /// when they share route, direction, stops, exactness flags, and
    /// shape. How long they take is the trips' own business.
    #[test]
    fn patterns_are_stop_sequences(trips in trips_strategy()) {
        let net = transit_codec::import(&build_feed(&trips)).network;
        let key = |t: &GenTrip| {
            let flags: Vec<bool> = rows_of(t).iter().map(|r| r.timepoint.unwrap_or(true)).collect();
            (t.route, t.direction, t.stops.clone(), flags, t.shape)
        };
        let pattern_of = |i: usize| find(&net, &format!("t{i}")).map(|(_, pt, _)| pt.pattern.clone());
        for a in 0..trips.len() {
            for b in a + 1..trips.len() {
                let same = key(&trips[a]) == key(&trips[b]);
                prop_assert_eq!(
                    pattern_of(a) == pattern_of(b),
                    same,
                    "t{} and t{}: sharing a pattern must mean sharing a stop sequence",
                    a,
                    b
                );
            }
        }
    }

    /// Blocks keep every trip the feed gives them, by id, in the order
    /// they run — including trips on different services under one
    /// block id.
    #[test]
    fn blocks_keep_their_trips(trips in trips_strategy()) {
        let net = transit_codec::import(&build_feed(&trips)).network;
        let mut expected: BTreeMap<String, Vec<(u32, String)>> = BTreeMap::new();
        for (i, b) in assign_blocks(&trips).into_iter().enumerate() {
            if let Some(b) = b {
                expected.entry(b).or_default().push((trips[i].start, format!("t{i}")));
            }
        }
        let got: BTreeMap<String, BTreeSet<String>> = net
            .blocks
            .iter()
            .map(|(b, block)| (b.clone(), block.trips.iter().map(|r: &TripRef| r.trip.clone()).collect()))
            .collect();
        let want: BTreeMap<String, BTreeSet<String>> =
            expected.iter().map(|(b, ts)| (b.clone(), ts.iter().map(|(_, id)| id.clone()).collect())).collect();
        prop_assert_eq!(got, want);
    }
}
