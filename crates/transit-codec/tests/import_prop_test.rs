//! The inference law: `gtfs_to_network` is valid BY CONSTRUCTION over
//! every SPEC-VALID feed — the inferred document passes the model's
//! own validation, so a planner can always edit a derived network
//! immediately (this broke on the real C-TRAN feed: 187 issues, the
//! derived document unloadable). The generator is adversarial toward
//! the inference's failure modes while staying inside the GTFS spec:
//! colliding trip starts, trip ids whose lexical order ignores time,
//! per-trip run-time variation inside one pattern, shared blocks, and
//! the spec-legal-but-unrepresentable shapes (the same derived trip
//! claimed by two blocks, two trips of one block at the same second).
//! Garbage beyond the spec is the panic-fuzz layer's job
//! (panic_fuzz_test.rs); this law assumes a well-formed feed.
//!
//! Two laws:
//!
//! 1. `validation_issues(gtfs_to_network(feed))` is empty — always.
//!    Validation is the gate that blocks *authored* edits; the
//!    bootstrap path must never hand the planner a document the gate
//!    rejects.
//! 2. The import is deterministic — the same feed yields the same
//!    document on every host (the sync thesis needs that).
//! 3. The trip-id map is the honest join (see the test below).
//! 4. Stop times survive the import: every kept trip's per-stop
//!    times are the ones the model materializes for its derived
//!    trip (see the test below).
//! 5. The round trip is lossless: import the feed, export the
//!    document, and on every service date the same trips run (see
//!    the test below).

use std::io::Write;

use proptest::prelude::*;

/// One generated trip. Everything about it is adversarial by
/// default: starts come from a small pool (collisions are the point),
/// the per-trip `leg` varies run time inside one pattern, and blocks
/// come from a small pool so chains overlap and double-claim. Trip
/// ids are assigned per index in `gen_feed` (unique — the spec's
/// primary key) and their lexical order ignores start time.
#[derive(Clone, Debug)]
struct GenTrip {
    route: u8,
    service: u8,
    block: Option<u8>,
    /// Start as a 10-minute slot index (0..=96 → 00:00..16:00).
    start_slot: u16,
    /// Ordered distinct stop indices (2..=4 of the 6 stops).
    stops: Vec<u8>,
    /// Minutes between stops (1..=30): run-time variation.
    leg_min: u8,
}

fn gen_trip() -> impl Strategy<Value = GenTrip> {
    (
        0u8..2, // route
        0u8..2, // service key
        proptest::option::of(0u8..2), // block pool of 2
        0u16..96, // start slot
        proptest::collection::vec(0u8..6, 2..=4), // raw stop picks
        1u8..30, // leg minutes
    )
        .prop_map(|(route, service, block, start_slot, stops, leg_min)| {
            // Distinct, order-preserving stop sequence.
            let mut seq: Vec<u8> = stops.into_iter().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
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

    let csv = |rows: Vec<String>| rows.join("\n");
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
    add(
        &mut w,
        "trips.txt",
        &csv(
            std::iter::once("route_id,service_id,trip_id,block_id".into())
                .chain(trips.iter().enumerate().map(|(i, t)| {
                    format!(
                        "{},{},{},{},",
                        t.route,
                        t.service,
                        // Index ids: unique (the spec's primary key),
                        // and lexically unordered against start times.
                        format!("t{i}"),
                        t.block.map(|b| b.to_string()).unwrap_or_default()
                    )
                }))
                .collect(),
        ),
    );
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Law 1 (and 2): any feed imports to a document the model itself
    /// accepts, and imports deterministically.
    #[test]
    fn imported_network_is_valid_by_construction(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let bytes = gen_feed(&trips);
        let gtfs = transit_core::Gtfs::from_zip_bytes(&bytes)
            .expect("generated feeds parse");
        let outcome = transit_codec::gtfs_to_network(&gtfs);
        let issues = transit_network::validation_issues(&outcome.network);
        prop_assert!(
            issues.is_empty(),
            "the model rejects its own import: {:?}",
            issues
        );
        // Determinism: same feed bytes, same document AND same
        // resolution notes, in the same order.
        let again = transit_codec::gtfs_to_network(&gtfs);
        prop_assert_eq!(outcome.network, again.network);
        prop_assert_eq!(outcome.notes, again.notes);
        prop_assert_eq!(outcome.trip_map, again.trip_map);
    }

    /// Law 3: the trip-id map is the HONEST join — every kept agency
    /// trip id appears exactly once, every entry names a derived
    /// trip the network actually materializes, and the derived
    /// identity agrees with the id the export mints for that trip
    /// (`"{schedule}@{start}"`). Anything less and the map is a
    /// guess, not a join.
    #[test]
    fn trip_id_map_joins_every_kept_trip(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&gen_feed(&trips))
            .expect("generated feeds parse");
        let outcome = transit_codec::gtfs_to_network(&gtfs);
        let mut seen = std::collections::BTreeSet::new();
        for entry in &outcome.trip_map.entries {
            let pattern_trips = outcome
                .network
                .pattern_trips
                .get(&entry.pattern_trips)
                .unwrap_or_else(|| panic!("map names unknown schedule {}", entry.pattern_trips));
            let starts = match &pattern_trips.departures {
                transit_network::Departures::ExactTimes { starts } => starts,
                transit_network::Departures::HeadwayBands { .. } =>
                    panic!("import mints exact-times schedules only"),
            };
            prop_assert!(
                starts.contains(&entry.start_secs),
                "map names a start the schedule does not carry"
            );
            for id in &entry.agency_trip_ids {
                prop_assert!(seen.insert(id.clone()), "agency id {id} mapped twice");
            }
        }
        // The generator only emits spec-valid trips the import keeps,
        // so coverage must be total: every agency trip id in the feed.
        let expected: std::collections::BTreeSet<String> =
            gtfs.trips.keys().cloned().collect();
        prop_assert_eq!(seen, expected, "the map dropped agency trip ids");
        // The export mints "{schedule}@{start}": the map's derived
        // identity and the published artifact's ids agree exactly.
        let exported = transit_codec::network_to_gtfs(&outcome.network)
            .expect("a valid document exports");
        let minted: std::collections::BTreeSet<&str> =
            exported.trips.keys().map(String::as_str).collect();
        for entry in &outcome.trip_map.entries {
            let derived = format!("{}@{}", entry.pattern_trips, entry.start_secs);
            prop_assert!(
                minted.contains(derived.as_str()),
                "map's derived id {derived} is not an id the export mints"
            );
        }
    }

    /// Law 4: stop times survive the import. For every kept agency
    /// trip, the derived trip the map names materializes the feed's
    /// own per-stop times — same stops, same order, same times. The
    /// generator varies run time per trip inside one stop sequence
    /// (the defect's shape: the C-TRAN import collapsed each
    /// pattern to its shortest member's runtime and the router
    /// planned boardings 9–11 minutes fast that did not exist in
    /// the feed), so this law holds only if that variation is
    /// carried, as pattern variants.
    #[test]
    fn materialized_trips_preserve_feed_stop_times(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&gen_feed(&trips))
            .expect("generated feeds parse");
        let outcome = transit_codec::gtfs_to_network(&gtfs);
        for entry in &outcome.trip_map.entries {
            let derived = transit_network::materialize_service(&outcome.network, &entry.pattern_trips)
                .expect("a valid document materializes")
                .into_iter()
                .find(|t| t.start_secs == entry.start_secs)
                .unwrap_or_else(|| {
                    panic!("map names a start {} does not run", entry.pattern_trips)
                });
            for id in &entry.agency_trip_ids {
                let rows = gtfs.trip_stop_times(id).expect("kept trip has rows");
                prop_assert_eq!(
                    derived.stop_times.len(),
                    rows.len(),
                    "trip {}: a stop was lost or invented",
                    id
                );
                for (st, row) in derived.stop_times.iter().zip(&rows) {
                    prop_assert_eq!(&st.stop, &row.stop_id, "trip {}: stop order changed", id);
                    // The generator authors arrival == departure, so
                    // BOTH survive — a model that collapses run times
                    // fails here, not just on arrival.
                    let t = row.arrival_time.or(row.departure_time)
                        .unwrap_or_else(|| panic!("trip {}: row without any time", id));
                    prop_assert_eq!(
                        (st.arrival_secs, st.departure_secs),
                        (t, t),
                        "trip {} at {}: the model materialized a time the feed does not carry",
                        id,
                        row.stop_id
                    );
                }
            }
        }
    }
    /// Law 5: the round trip is lossless. Import the feed, export
    /// the document, and on every service date the SCHEDULE is
    /// unchanged: the same trips — (route, stop sequence with its
    /// per-stop times) — run on that date. Trip and service ids are
    /// minted on the way out (the model's identity is (schedule,
    /// start); the export's service_id is the schedule id), and
    /// rider-identical duplicate trips collapse to one derived
    /// trip, so the law compares the deduplicated SET of trips per
    /// date — anything the model invented, altered, or dropped
    /// fails here.
    #[test]
    fn round_trip_reproduces_the_feed_schedule(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let gtfs = transit_core::Gtfs::from_zip_bytes(&gen_feed(&trips))
            .expect("generated feeds parse");
        let network = transit_codec::gtfs_to_network(&gtfs).network;
        let exported = transit_codec::network_to_gtfs(&network).expect("valid documents export");
        let again = transit_core::Gtfs::from_zip_bytes(&transit_core::write::to_zip_bytes(&exported))
            .expect("the exported feed parses");

        // What a rider can board on `date`: the set of (route, stop
        // sequence with times) the feed runs that day, deduplicated
        // (the model's trip identity collapses rider-identical
        // duplicates — that loss is by design and invisible).
        let day_schedule = |feed: &transit_core::Gtfs, date: chrono::NaiveDate| {
            let owned = feed.active_services(date);
            let active: std::collections::BTreeSet<&str> =
                owned.iter().map(|s| s.as_str()).collect();
            let mut out: std::collections::BTreeSet<(String, Vec<(String, u32, u32)>)> =
                std::collections::BTreeSet::new();
            for (id, trip) in &feed.trips {
                if !active.contains(trip.service_id.as_str()) {
                    continue;
                }
                let rows = feed.trip_stop_times(id).expect("trip rows");
                let seq = rows
                    .iter()
                    .map(|r| {
                        (
                            r.stop_id.clone(),
                            r.arrival_time.or(r.departure_time).expect("generated rows carry times"),
                            r.departure_time.or(r.arrival_time).expect("generated rows carry times"),
                        )
                    })
                    .collect();
                out.insert((trip.route_id.clone(), seq));
            }
            out
        };

        // One weekday and one weekend probe: the generator's two
        // calendars. The original carries regular calendar rows,
        // the export exception-based services — active_services
        // resolves both shapes, so the law does not care.
        for date in ["2026-10-23", "2026-10-25"] {
            let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
            prop_assert_eq!(
                day_schedule(&gtfs, d),
                day_schedule(&again, d),
                "the round trip changed the schedule on {}",
                date
            );
        }
    }
}
