//! The codec fixpoint: `export ∘ import` is the identity on the
//! image of import — the publish path. A planner imports a feed,
//! edits the document, and publishes; the published artifact must
//! represent the SAME network the planner sees. The generator is
//! adversarial (colliding starts, run-time variation, overlapping
//! blocks), so the round trip is exercised on exactly the feeds
//! inference is worst at.
//!
//! The API these laws require: `transit_codec::import(&Feed) ->
//! ImportOutcome { network, notes, boundary }` and
//! `transit_codec::export(&Network, &Boundary) -> Result<Feed, String>`
//! (`Boundary` is the import's serialization residue — stop_sequence
//! numbering; `Boundary::default()` for an authored network).
//!
//! Laws:
//!
//! * **Document fixpoint** — `import(export(import(F)))` equals
//!   `import(F)`: the network, ids included, and the boundary. No
//!   exemptions: service ids, calendars, and trip ids all survive.
//! * **Semantic fixpoint** — the materialized trips on every probed
//!   date — weekday, weekend, out of range — survive exactly: same
//!   trip ids, routes, times, stop visits.
//! * **Determinism** — the round trip is a pure function.
//! * **Three views, one document** — the runcut, the materialized
//!   day, and the published artifact agree: every block trip is a
//!   trip the day runs, every materialized trip is published under
//!   its own id with its exact times, and the block a trip belongs to
//!   is the block the export says runs it.

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

/// The materialized trips of a document on `date`, with their ids.
fn trips_of(
    net: &transit_network::Network,
    date: NaiveDate,
) -> Vec<(String, String, u32, Vec<(String, u32, u32)>)> {
    let mut trips: Vec<_> = transit_network::materialize(net, date)
        .expect("valid documents materialize")
        .into_iter()
        .map(|t| {
            (
                t.trip,
                t.route,
                t.start_secs,
                t.stop_times.into_iter().map(|st| (st.stop, st.arrival_secs, st.departure_secs)).collect(),
            )
        })
        .collect();
    trips.sort();
    trips
}

fn import(bytes: &[u8]) -> transit_codec::ImportOutcome {
    transit_codec::import(&transit_codec::Feed::from_zip_bytes(bytes).expect("generated feeds read"))
}

fn round_trip(first: &transit_codec::ImportOutcome) -> transit_codec::ImportOutcome {
    let feed = transit_codec::export(&first.network, &first.boundary).expect("a valid document exports");
    transit_codec::import(&transit_codec::Feed::from_zip_bytes(&feed.to_zip_bytes()).expect("exports read"))
}

/// GTFS time text → seconds (the oracle's own reader).
fn secs(text: &str) -> u32 {
    let p: Vec<u32> = text.split(':').map(|x| x.parse().unwrap()).collect();
    p[0] * 3600 + p[1] * 60 + p[2]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Document fixpoint: the re-import is the import, exactly.
    #[test]
    fn round_trip_preserves_the_document(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let first = import(&gen_feed(&trips));
        let issues = transit_network::validation_issues(&first.network);
        prop_assert!(issues.is_empty(), "the model rejects its own import: {:?}", issues);
        let second = round_trip(&first);
        prop_assert_eq!(&second.network, &first.network);
        prop_assert_eq!(&second.boundary, &first.boundary);
    }

    /// Semantic fixpoint: the materialized trips survive the round
    /// trip on every probed date — weekday, weekend, and out of range.
    #[test]
    fn materialized_trips_survive_the_round_trip(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let first = import(&gen_feed(&trips));
        let second = round_trip(&first);
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
    fn round_trip_is_deterministic(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let first = import(&gen_feed(&trips));
        let a = transit_codec::export(&first.network, &first.boundary).unwrap().to_zip_bytes();
        let b = transit_codec::export(&first.network, &first.boundary).unwrap().to_zip_bytes();
        prop_assert_eq!(a, b);
        let (x, y) = (round_trip(&first), round_trip(&first));
        prop_assert_eq!(x.network, y.network);
        prop_assert_eq!(x.notes, y.notes);
    }

    /// Three views of ONE document agree: every materialized trip is
    /// published under its own id with exactly its times, and every
    /// block trip is a trip the day runs, published under that block.
    #[test]
    fn runcut_day_and_export_agree(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let outcome = import(&gen_feed(&trips));
        let net = &outcome.network;
        let feed = transit_codec::export(net, &outcome.boundary).expect("a valid document exports");

        let mut day: std::collections::BTreeMap<String, Vec<(String, u32, u32)>> = Default::default();
        for key in net.pattern_trips.keys() {
            for trip in transit_network::materialize_service(net, key).expect("valid materializes") {
                day.insert(
                    trip.trip.clone(),
                    trip.stop_times.iter().map(|st| (st.stop.clone(), st.arrival_secs, st.departure_secs)).collect(),
                );
            }
        }
        let mut published: std::collections::BTreeMap<String, Vec<(u32, String, u32, u32)>> = Default::default();
        for row in &feed.stop_times {
            published.entry(row.trip_id.clone().unwrap()).or_default().push((
                row.stop_sequence.as_deref().unwrap().parse().unwrap(),
                row.stop_id.clone().unwrap(),
                secs(row.arrival_time.as_deref().expect("the generator's rows are all timed")),
                secs(row.departure_time.as_deref().expect("the generator's rows are all timed")),
            ));
        }
        let published: std::collections::BTreeMap<String, Vec<(String, u32, u32)>> = published
            .into_iter()
            .map(|(id, mut rows)| {
                rows.sort();
                (id, rows.into_iter().map(|(_, s, a, d)| (s, a, d)).collect())
            })
            .collect();
        prop_assert_eq!(&published, &day, "the export publishes exactly the day's trips, by id, with their times");

        let block_of: std::collections::BTreeMap<String, Option<String>> =
            feed.trips.iter().map(|t| (t.trip_id.clone().unwrap(), t.block_id.clone())).collect();
        for (block_id, block) in &net.blocks {
            for r in &block.trips {
                prop_assert!(day.contains_key(&r.trip), "block {} chains a trip the day does not run", block_id);
                prop_assert_eq!(
                    block_of[&r.trip].as_deref(),
                    Some(block_id.as_str()),
                    "the export disagrees with the runcut about {}",
                    r.trip
                );
            }
        }
    }
}
