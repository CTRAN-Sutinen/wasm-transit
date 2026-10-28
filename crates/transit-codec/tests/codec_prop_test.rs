//! The codec fixpoint: `network_to_gtfs ∘ gtfs_to_network` is the
//! identity on the image of import — the publish path. A planner
//! imports a feed, edits the document, and publishes; the published
//! artifact must represent the SAME network the planner sees. The
//! generator is the adversarial import generator of
//! `import_prop_test.rs` (colliding starts, run-time variation,
//! overlapping blocks), so the round trip is exercised on exactly
//! the feeds inference is worst at.
//!
//! Laws:
//!
//! * **Document fixpoint** — `import(export(import(F)))` equals
//!   `import(F)`, document-equal (ids included). Anything that
//!   fails here is a genuine codec asymmetry, not a test artifact.
//! * **Semantic fixpoint** — even where document equality is too
//!   strict (documented inference losses), the materialized trips
//!   on every probed date — weekday, weekend, out of range — must
//!   survive the round trip exactly: same routes, same times, same
//!   stop visits. This is the planner's actual product.
//! * **Determinism** — the round trip is a pure function.
//! * **Three views, one document** — the runcut, the materialized
//!   day, and the published artifact agree: every block trip is a
//!   trip the day runs, every materialized trip is a trip the
//!   export publishes with its exact times, and the block a trip
//!   belongs to is the block the export says runs it (the old
//!   validate/runcut desync would have failed this law).

use std::io::Write;

use chrono::{Duration, NaiveDate};
use proptest::prelude::*;

const EPOCH: NaiveDate = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

#[derive(Clone, Debug)]
struct GenTrip {
    route: u8,
    service: u8,
    block: Option<u8>,
    start_slot: u16,
    stops: Vec<u8>,
    leg_min: u8,
    /// GTFS direction_id (or none), and a headsign from a small pool
    /// (or none) — mixed within a direction, so the naming tie-break
    /// and the no-headsign fallback both run.
    direction: Option<u8>,
    headsign: Option<u8>,
}

fn gen_trip() -> impl Strategy<Value = GenTrip> {
    (
        0u8..2,
        0u8..2,
        proptest::option::of(0u8..2),
        0u16..96,
        proptest::collection::vec(0u8..6, 2..=4),
        1u8..30,
        proptest::option::of(0u8..2),
        proptest::option::of(0u8..3),
    )
        .prop_map(|(route, service, block, start_slot, stops, leg_min, direction, headsign)| {
            let mut seq: Vec<u8> = stops
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if seq.len() < 2 {
                seq = vec![0, 1];
            }
            GenTrip { route, service, block, start_slot, stops: seq, leg_min, direction, headsign }
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

    let mut trips_txt = String::from("route_id,service_id,trip_id,block_id,direction_id,trip_headsign");
    for (i, t) in trips.iter().enumerate() {
        trips_txt.push_str(&format!(
            "\n{},{},{},{},{},{}",
            t.route,
            t.service,
            format!("t{i}"),
            t.block.map(|b| b.to_string()).unwrap_or_default(),
            t.direction.map(|d| d.to_string()).unwrap_or_default(),
            t.headsign.map(|h| format!("Sign {h}")).unwrap_or_default()
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

/// The materialized trips of a document on `date`, as comparable
/// semantic tuples: id strings are replaced by what they MEAN (route,
/// times, stop visits), so minted ids cannot mask real differences.
fn trips_of(
    net: &transit_network::Network,
    date: NaiveDate,
) -> Vec<(String, u32, Vec<(String, u32, u32)>)> {
    let mut trips: Vec<(String, u32, Vec<(String, u32, u32)>)> =
        transit_network::materialize(net, date)
            .expect("valid documents materialize")
            .into_iter()
            .map(|t| {
                (
                    t.route,
                    t.start_secs,
                    t.stop_times
                        .into_iter()
                        .map(|st| (st.stop, st.arrival_secs, st.departure_secs))
                        .collect(),
                )
            })
            .collect();
    trips.sort();
    trips
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Document fixpoint, at the export contract's strength. The
    /// export deliberately expands calendars to explicit per-date
    /// rows ("exact for any authored rule" — see the module docs),
    /// so re-import mints exception-only service types with fresh
    /// ids. Everything else must survive the round trip exactly:
    /// name, plan, anchor, stops, routes, patterns, blocks, and the
    /// schedules pairwise (pattern + starts); the calendars must
    /// survive semantically: each schedule's service runs on the
    /// same dates of the whole probed year.
    #[test]
    fn round_trip_preserves_the_document(
        trips in proptest::collection::vec(gen_trip(), 0..=14),
    ) {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let first = transit_codec::gtfs_to_network(&gtfs);
        prop_assert!(
            transit_network::validation_issues(&first.network).is_empty(),
            "the model rejects its own import: {:?}",
            transit_network::validation_issues(&first.network)
        );

        let exported = transit_codec::network_to_gtfs(&first.network)
            .expect("a valid document exports");
        let second = transit_codec::gtfs_to_network(&exported);

        prop_assert!(
            transit_network::validation_issues(&second.network).is_empty(),
            "the round trip yields a document the model rejects: {:?}",
            transit_network::validation_issues(&second.network)
        );

        let a = &first.network;
        let b = &second.network;
        prop_assert_eq!(&b.name, &a.name);
        prop_assert_eq!(&b.plan, &a.plan);
        prop_assert_eq!(&b.anchor, &a.anchor);
        prop_assert_eq!(&b.stops, &a.stops);
        prop_assert_eq!(&b.routes, &a.routes);
        prop_assert_eq!(&b.patterns, &a.patterns);

        // Schedule ids are minted per import (and re-minted on the
        // round trip), so they cannot be part of the contract. The
        // canonical form of a schedule is (pattern, schedule value).
        let canon = |net: &transit_network::Network| -> std::collections::HashMap<String, String> {
            net.pattern_trips
                .iter()
                .map(|(id, s)| {
                    (
                        id.clone(),
                        format!("{}|{}", s.pattern, serde_json::to_string(&s.departures).unwrap()),
                    )
                })
                .collect()
        };
        let blocks_canon = |net: &transit_network::Network| -> std::collections::BTreeMap<
            String,
            (Vec<(String, u32)>, String),
        > {
            let c = canon(net);
            net.blocks
                .iter()
                .map(|(id, block)| {
                    (
                        id.clone(),
                        (
                            block
                                .trips
                                .iter()
                                .map(|t| (c[&t.pattern_trips].clone(), t.start_secs))
                                .collect(),
                            serde_json::to_string(&block.layovers).unwrap(),
                        ),
                    )
                })
                .collect()
        };
        prop_assert_eq!(&blocks_canon(b), &blocks_canon(a), "blocks diverge");

        // Schedules pairwise by canonical form — minted schedule and
        // service ids cannot be part of the contract.
        let mut scheds = |net: &transit_network::Network| {
            let mut list: Vec<String> = canon(net).into_values().collect();
            list.sort();
            list
        };
        let sa = scheds(a);
        let sb = scheds(b);
        prop_assert_eq!(sb, sa, "schedules diverge");

        // Calendar semantics: each paired schedule's service type
        // runs on the same dates of the probed year. Pair by pattern
        // and starts, then compare the rules they reference.
        let by_pattern = |net: &transit_network::Network| -> Vec<(String, String)> {
            let mut list: Vec<(String, String)> = net
                .pattern_trips
                .values()
                .map(|s| (s.pattern.clone(), s.service_type.clone()))
                .collect();
            list.sort();
            list
        };
        let pa = by_pattern(a);
        let pb = by_pattern(b);
        prop_assert_eq!(pa.len(), pb.len());
        for ((pattern, type_a), (_, type_b)) in pa.iter().zip(&pb) {
            let rule_a = &a.service_types[type_a].rule;
            let rule_b = &b.service_types[type_b].rule;
            for day in 0..365 {
                let date = EPOCH + Duration::days(day);
                prop_assert_eq!(
                    rule_a.runs_on(date),
                    rule_b.runs_on(date),
                    "service of pattern {} diverges on {}",
                    pattern,
                    date
                );
            }
        }
    }

    /// Semantic fixpoint: the materialized trips survive the round
    /// trip on every probed date — weekday, weekend, and out of range.
    #[test]
    fn materialized_trips_survive_the_round_trip(
        trips in proptest::collection::vec(gen_trip(), 0..=14),
    ) {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let first = transit_codec::gtfs_to_network(&gtfs);
        let exported = transit_codec::network_to_gtfs(&first.network)
            .expect("a valid document exports");
        let second = transit_codec::gtfs_to_network(&exported);

        for day in [0i64, 4, 9, 400] {
            let date = EPOCH + Duration::days(day);
            prop_assert_eq!(
                trips_of(&second.network, date),
                trips_of(&first.network, date),
                "materialized trips diverge on {}",
                date
            );
        }
    }

    /// Determinism: the round trip is a pure function of the feed.
    #[test]
    fn round_trip_is_deterministic(
        trips in proptest::collection::vec(gen_trip(), 0..=14),
    ) {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let first = transit_codec::gtfs_to_network(&gtfs);
        let exported = transit_codec::network_to_gtfs(&first.network)
            .expect("a valid document exports");
        let a = transit_codec::gtfs_to_network(&exported);
        let b = transit_codec::gtfs_to_network(&exported);
        prop_assert_eq!(a.network, b.network);
        prop_assert_eq!(a.notes, b.notes);
    }

    /// QAF-20's missing cross-capability check: the runcut, the
    /// materialized day, and the published artifact are three views
    /// of ONE document, so they must agree — every block trip is a
    /// trip the day actually runs, every materialized trip is
    /// published with its exact stop sequence and times, and the
    /// block a trip belongs to is the block the export says runs it.
    /// A desync between what the runcut runs and what the export
    /// publishes fails here instead of in a planner's session.
    #[test]
    fn runcut_day_and_export_agree(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&gen_feed(&trips)).expect("parse");
        let net = transit_codec::gtfs_to_network(&gtfs).network;
        let exported = transit_codec::network_to_gtfs(&net).expect("a valid document exports");

        // The day: every materialized (schedule, start) with its
        // per-stop times.
        let mut day: std::collections::BTreeMap<(String, u32), Vec<(String, u32, u32)>> =
            Default::default();
        for id in net.pattern_trips.keys() {
            for trip in transit_network::materialize_service(&net, id).expect("valid materializes") {
                day.insert(
                    (id.clone(), trip.start_secs),
                    trip.stop_times
                        .iter()
                        .map(|st| (st.stop.clone(), st.arrival_secs, st.departure_secs))
                        .collect(),
                );
            }
        }
        // The artifact: the export's trips, keyed by the derived id
        // it mints ("{schedule}@{start}").
        let mut published: std::collections::BTreeMap<String, Vec<(String, u32, u32)>> =
            Default::default();
        for trip_id in exported.trips.keys() {
            published.insert(
                trip_id.clone(),
                exported
                    .trip_stop_times(trip_id)
                    .expect("exported trip rows")
                    .iter()
                    .map(|st| {
                        (
                            st.stop_id.clone(),
                            st.arrival_time.expect("export writes arrivals"),
                            st.departure_time.expect("export writes departures"),
                        )
                    })
                    .collect(),
            );
        }
        prop_assert_eq!(day.len(), published.len(), "the export lost or invented trips");
        for ((sched, start), times) in &day {
            let id = format!("{sched}@{start}");
            prop_assert!(published.contains_key(&id), "the export does not publish {id}");
            prop_assert_eq!(&published[&id], times, "the export retimed {}", id);
        }
        // The runcut: chains reference real trips, and the published
        // artifact agrees about which bus runs them.
        for (block_id, block) in &net.blocks {
            for trip in &block.trips {
                prop_assert!(
                    day.contains_key(&(trip.pattern_trips.clone(), trip.start_secs)),
                    "block {block_id} chains a trip the day does not run"
                );
                let id = format!("{}@{}", trip.pattern_trips, trip.start_secs);
                prop_assert_eq!(
                    exported.trips[&id].block_id.as_deref(),
                    Some(block_id.as_str()),
                    "the export disagrees with the runcut about {}",
                    id
                );
            }
        }
    }
}
