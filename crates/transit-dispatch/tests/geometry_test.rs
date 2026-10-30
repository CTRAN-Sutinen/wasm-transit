//! Where service runs today: a trip that follows its pattern draws the
//! authored alignment; a diverted trip draws the alignment to its
//! first kept stop, the diversion's traced detour, and the alignment
//! from its last kept stop (QAF-50) — a skip-only diversion leaves the
//! alignment as it is; the ridden segment slices that line between
//! its stops.

use chrono::NaiveDate;
use transit_dispatch::{patch_day, trip_path, trip_segment};
use transit_network::{materialize, Alignment, AlignmentPoint, Coord, MaterializedTrip, Network};
use transit_sync::{DetourPoint, Diversion};

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
    assert_eq!(trip_path(&net, &d, &[], "10-weekday@28800", 28800).len(), 5);
    assert!(trip_path(&net, &d, &[], "10-weekday@28800", 1).is_empty(), "a trip that does not run draws nothing");
    let seg = trip_segment(&net, &d, &[], "10-weekday@28800", 28800, "B", "C");
    assert_eq!(seg, vec![c(45.51, -73.51), c(45.515, -73.51), c(45.52, -73.52)], "sliced at the nearest vertices");
}

fn diversion(first: &str, last: &str, skipped: &[&str], detour: Option<&[Coord]>) -> Diversion {
    Diversion {
        id: "d1".into(),
        route_id: "10".into(),
        direction_id: None,
        first_kept_stop: first.into(),
        last_kept_stop: last.into(),
        skipped_stops: skipped.iter().map(|s| s.to_string()).collect(),
        detour: detour.map(|pts| pts.iter().map(|p| DetourPoint { lat: p.lat, lon: p.lon }).collect()),
        added_secs: 0,
        start_date: date(),
        end_date: date(),
    }
}

/// The traced detour around B.
fn around_b() -> Vec<Coord> {
    vec![c(45.50, -73.49), c(45.52, -73.49)]
}

#[test]
fn a_skip_only_diversion_keeps_the_alignment() {
    // `detour: None` = the alignment unchanged: the bus drives the
    // same streets past the stop it skips.
    let net = network();
    let skip_b = [diversion("A", "C", &["B"], None)];
    let d = patch_day(&day(&net), &skip_b);
    let whole = trip_path(&net, &day(&net), &[], "10-weekday@28800", 28800);
    assert_eq!(trip_path(&net, &d, &skip_b, "10-weekday@28800", 28800), whole);
    assert_eq!(trip_segment(&net, &d, &skip_b, "10-weekday@28800", 28800, "A", "C"), whole);
    assert_eq!(
        trip_segment(&net, &d, &skip_b, "10-weekday@28800", 28800, "A", "B"),
        vec![c(45.5, -73.5), c(45.51, -73.51)],
        "a stop the trip no longer visits falls back to the straight line"
    );
    assert!(trip_segment(&net, &d, &skip_b, "10-weekday@28800", 28800, "A", "Z").is_empty());
}

#[test]
fn a_detour_splices_into_the_alignment() {
    let net = network();
    let line = trip_path(&net, &day(&net), &[], "10-weekday@28800", 28800);

    // A → C around B: the alignment to A, the detour, the alignment
    // from C.
    let around = [diversion("A", "C", &["B"], Some(&around_b()))];
    let d = patch_day(&day(&net), &around);
    let mut expected = vec![line[0]];
    expected.extend(around_b());
    expected.push(line[4]);
    assert_eq!(trip_path(&net, &d, &around, "10-weekday@28800", 28800), expected);
    assert_eq!(trip_segment(&net, &d, &around, "10-weekday@28800", 28800, "A", "C"), expected, "the ride follows the detour");

    // A → B with nothing skipped (a detour between two kept stops):
    // the alignment from B's vertex on.
    let ab = [diversion("A", "B", &[], Some(&around_b()))];
    let d = patch_day(&day(&net), &ab);
    let mut expected = vec![line[0]];
    expected.extend(around_b());
    expected.extend_from_slice(&line[2..]);
    assert_eq!(trip_path(&net, &d, &ab, "10-weekday@28800", 28800), expected);

    // Another route's diversion draws nothing on this trip.
    let mut other = diversion("A", "C", &["B"], Some(&around_b()));
    other.route_id = "20".into();
    assert_eq!(trip_path(&net, &day(&net), &[other], "10-weekday@28800", 28800), line);
}

#[test]
fn without_an_alignment_a_diverted_trip_joins_its_stops_around_the_detour() {
    let mut net = network();
    net.patterns.get_mut("10-inbound").unwrap().alignment = None;
    let skip_b = [diversion("A", "C", &["B"], None)];
    let d = patch_day(&day(&net), &skip_b);
    assert_eq!(trip_path(&net, &d, &skip_b, "10-weekday@28800", 28800), vec![c(45.5, -73.5), c(45.52, -73.52)]);
    let around = [diversion("A", "C", &["B"], Some(&around_b()))];
    let d = patch_day(&day(&net), &around);
    let mut expected = vec![c(45.5, -73.5)];
    expected.extend(around_b());
    expected.push(c(45.52, -73.52));
    assert_eq!(trip_path(&net, &d, &around, "10-weekday@28800", 28800), expected);
}
