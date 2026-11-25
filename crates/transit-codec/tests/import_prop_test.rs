//! Over every spec-valid feed (collisions in trip starts, trip ids that
//! ignore time, run-time variation inside a pattern, shared blocks):
//!
//! 1. `validation_issues(import(feed))` is empty, so a planner can edit a
//!    derived network at once.
//! 2. The same feed yields the same outcome on every host.
//!
//! Garbage beyond the spec is `panic_fuzz_test.rs`.

use std::io::Write;

use proptest::prelude::*;

/// Starts come from a small pool, so collisions are intended. Blocks come
/// from a small pool, so chains are long.
#[derive(Clone, Debug)]
struct GenTrip {
    route: u8,
    service: u8,
    block: Option<u8>,
    /// A 10-minute slot index (0..=96 is 00:00..16:00).
    start_slot: u16,
    /// Distinct, in order: 2..=4 of the 6 stops.
    stops: Vec<u8>,
    /// Minutes between stops (1..=30).
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
            let mut seq: Vec<u8> = stops.into_iter().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            if seq.len() < 2 {
                seq = vec![0, 1];
            }
            GenTrip { route, service, block, start_slot, stops: seq, leg_min }
        })
}

/// The spec says one vehicle runs a block, so its trips on one service day
/// never overlap.
fn blocks(trips: &[GenTrip]) -> Vec<Option<u8>> {
    let mut taken: std::collections::BTreeMap<(u8, u8), Vec<(u32, u32)>> = Default::default();
    trips
        .iter()
        .map(|t| {
            let b = t.block?;
            let start = (t.start_slot as u32) * 600;
            let span = (start, start + (t.stops.len() as u32 - 1) * (t.leg_min as u32) * 60);
            let spans = taken.entry((b, t.service)).or_default();
            if spans.iter().any(|&(s, e)| span.0 <= e && s <= span.1) {
                return None;
            }
            spans.push(span);
            Some(b)
        })
        .collect()
}

fn gen_feed(trips: &[GenTrip]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };

    let csv = |rows: Vec<String>| rows.join("\n");
    let blocks = blocks(trips);
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
                        // Unique, and lexically unordered against start times.
                        format!("t{i}"),
                        blocks[i].map(|b| b.to_string()).unwrap_or_default()
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

    /// Properties 1 and 2.
    #[test]
    fn imported_network_is_valid_by_construction(trips in proptest::collection::vec(gen_trip(), 0..=14)) {
        let feed = transit_codec::Feed::from_zip_bytes(&gen_feed(&trips)).expect("generated feeds read");
        let outcome = transit_codec::import(&feed);
        let issues = transit_network::validation_issues(&outcome.network);
        prop_assert!(issues.is_empty(), "the model rejects its own import: {:?}", issues);
        prop_assert_eq!(transit_codec::import(&feed), outcome);
    }
}
