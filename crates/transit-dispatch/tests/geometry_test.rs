//! Where service runs today: a trip that follows its pattern draws the
//! authored alignment; a diverted trip draws its patched stops; the
//! ridden segment slices the alignment between its stops.

use chrono::NaiveDate;
use transit_dispatch::{patch_day, trip_path, trip_segment};
use transit_network::{materialize, Alignment, AlignmentPoint, Coord, MaterializedTrip, Network};
use transit_sync::Diversion;

fn c(lat: f64, lon: f64) -> Coord {
    Coord { lat, lon }
}

/// The planner network with an alignment bending between A and C.
fn network() -> Network {
    let mut net: Network = serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap();
    let line = [c(45.5, -73.5), c(45.505, -73.5), c(45.51, -73.51), c(45.515, -73.51), c(45.52, -73.52)];
    net.alignments.insert(
        "line".into(),
        Alignment { points: line.iter().map(|p| AlignmentPoint { lat: p.lat, lon: p.lon, distance: None }).collect() },
    );
    net.patterns.get_mut("10-inbound").unwrap().alignment = Some("line".into());
    net
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()
}

fn day(net: &Network) -> Vec<MaterializedTrip> {
    materialize(net, date()).unwrap()
}

#[test]
fn a_trip_on_its_pattern_draws_the_alignment() {
    let net = network();
    let d = day(&net);
    assert_eq!(trip_path(&net, &d, "10-weekday", 28800).len(), 5);
    assert!(trip_path(&net, &d, "10-weekday", 1).is_empty(), "a trip that does not run draws nothing");
    let seg = trip_segment(&net, &d, "10-weekday", 28800, "B", "C");
    assert_eq!(seg, vec![c(45.51, -73.51), c(45.515, -73.51), c(45.52, -73.52)], "sliced at the nearest vertices");
}

#[test]
fn a_diverted_trip_draws_its_patched_stops() {
    let net = network();
    let skip_b = Diversion {
        id: "d1".into(),
        route_id: "10".into(),
        direction_id: None,
        first_kept_stop: "A".into(),
        last_kept_stop: "C".into(),
        skipped_stops: vec!["B".into()],
        detour: None,
        added_secs: 0,
        start_date: date(),
        end_date: date(),
    };
    let d = patch_day(&day(&net), &[skip_b]);
    assert_eq!(trip_path(&net, &d, "10-weekday", 28800), vec![c(45.5, -73.5), c(45.52, -73.52)]);
    assert_eq!(trip_segment(&net, &d, "10-weekday", 28800, "A", "C"), vec![c(45.5, -73.5), c(45.52, -73.52)]);
    assert_eq!(
        trip_segment(&net, &d, "10-weekday", 28800, "A", "B"),
        vec![c(45.5, -73.5), c(45.51, -73.51)],
        "a stop the trip no longer visits falls back to the straight line"
    );
    assert!(trip_segment(&net, &d, "10-weekday", 28800, "A", "Z").is_empty());
}
