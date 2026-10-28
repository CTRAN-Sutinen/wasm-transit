//! The import's alignment laws: a feed's shapes ARE the network's
//! alignments — the feed's ids, every point in sequence order, every
//! published distance — and a trip's `shape_id` is the alignment its
//! pattern runs (the pattern points at the alignment). A shape
//! no trip uses is still an alignment; a shape several stop lists use
//! is one alignment.
//!
//! Written BEFORE the implementation, as the definition of the
//! behavior; the expected answers come from the generator's own
//! values, never from parsing or from the implementation. The feed is
//! built in memory (`transit_codec::Feed`), so no parser sits between
//! the generator and the import.
//!
//! The API these laws require, beyond `transit-network`'s alignment
//! laws (`crates/transit-network/tests/alignment_law_test.rs`):
//! `import(&Feed)` and `export(&Network, &Boundary)` as they stand.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use transit_codec::feed::{AgencyRow, CalendarRow, RouteRow, ShapeRow, StopRow, StopTimeRow, TripRow};
use transit_codec::Feed;

// ===================================================================
// Generator: spec-valid feeds.
// ===================================================================

/// Shape ids that look nothing like pattern ids.
const SHAPE_IDS: [&str; 4] = ["sh 7", "A/1", "0", "x_line"];

#[derive(Clone, Debug)]
struct GenShape {
    /// (lat, lon) in travel order.
    points: Vec<(f64, f64)>,
    /// The first shape_pt_sequence and the gap to each next one.
    first_seq: u32,
    gaps: Vec<u32>,
    with_distances: bool,
    /// Write the rows backwards: row order is not sequence order.
    reversed: bool,
}

impl GenShape {
    fn sequences(&self) -> Vec<u32> {
        let mut seq = self.first_seq;
        (0..self.points.len())
            .map(|i| {
                if i > 0 {
                    seq += self.gaps[i];
                }
                seq
            })
            .collect()
    }

    fn distance(&self, i: usize) -> Option<f64> {
        self.with_distances.then_some(i as f64 * 250.5)
    }
}

#[derive(Clone, Debug)]
struct GenTrip {
    /// Ordered distinct stops (2..=4 of s0..s5).
    stops: Vec<u8>,
    /// Index into the shapes.
    shape: Option<usize>,
}

#[derive(Clone, Debug)]
struct Spec {
    shapes: Vec<GenShape>,
    trips: Vec<GenTrip>,
}

fn gen_shape() -> impl Strategy<Value = GenShape> {
    (2usize..=5).prop_flat_map(|n| {
        (
            proptest::collection::vec(
                (-899_999i32..900_000, -1_799_999i32..1_800_000)
                    .prop_map(|(a, b)| (a as f64 / 1e4 + 0.000_012_345_67, b as f64 / 1e4 - 0.000_001_234_5)),
                n,
            ),
            0u32..3,
            proptest::collection::vec(1u32..6, n),
            any::<bool>(),
            any::<bool>(),
        )
            .prop_map(|(points, first_seq, gaps, with_distances, reversed)| GenShape {
                points,
                first_seq,
                gaps,
                with_distances,
                reversed,
            })
    })
}

fn spec_strategy() -> impl Strategy<Value = Spec> {
    proptest::collection::vec(gen_shape(), 0..=SHAPE_IDS.len()).prop_flat_map(|shapes| {
        let n = shapes.len();
        let trip = (
            (2usize..=4).prop_flat_map(|k| proptest::sample::subsequence((0u8..6).collect::<Vec<u8>>(), k)),
            if n == 0 { Just(None).boxed() } else { proptest::option::of(0..n).boxed() },
        )
            .prop_map(|(stops, shape)| GenTrip { stops, shape });
        (Just(shapes), proptest::collection::vec(trip, 1..=6)).prop_map(|(shapes, trips)| Spec { shapes, trips })
    })
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn build_feed(spec: &Spec) -> Feed {
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
    feed.routes.push(RouteRow { route_id: s("r"), route_short_name: s("r"), route_type: s("3"), ..Default::default() });
    feed.calendar.push(CalendarRow {
        service_id: s("wk"),
        monday: s("1"),
        tuesday: s("1"),
        wednesday: s("1"),
        thursday: s("1"),
        friday: s("1"),
        saturday: s("0"),
        sunday: s("0"),
        start_date: s("20260101"),
        end_date: s("20261231"),
    });
    for (k, shape) in spec.shapes.iter().enumerate() {
        let mut rows: Vec<ShapeRow> = shape
            .points
            .iter()
            .zip(shape.sequences())
            .enumerate()
            .map(|(i, (&(lat, lon), seq))| ShapeRow {
                shape_id: s(SHAPE_IDS[k]),
                shape_pt_lat: Some(lat.to_string()),
                shape_pt_lon: Some(lon.to_string()),
                shape_pt_sequence: Some(seq.to_string()),
                shape_dist_traveled: shape.distance(i).map(|d| d.to_string()),
            })
            .collect();
        if shape.reversed {
            rows.reverse();
        }
        feed.shapes.extend(rows);
    }
    for (i, t) in spec.trips.iter().enumerate() {
        let id = format!("t{i}");
        feed.trips.push(TripRow {
            route_id: s("r"),
            service_id: s("wk"),
            trip_id: Some(id.clone()),
            shape_id: t.shape.map(|k| SHAPE_IDS[k].to_string()),
            ..Default::default()
        });
        for (j, stop) in t.stops.iter().enumerate() {
            let at = format!("08:{:02}:00", j * 5);
            feed.stop_times.push(StopTimeRow {
                trip_id: Some(id.clone()),
                stop_id: Some(format!("s{stop}")),
                stop_sequence: Some((j + 1).to_string()),
                arrival_time: Some(at.clone()),
                departure_time: Some(at),
                ..Default::default()
            });
        }
    }
    feed
}

/// A shapes.txt row as values: (id, sequence, lat, lon, distance).
type ShapePoint = (String, u32, f64, f64, Option<f64>);

fn shape_points(feed: &Feed) -> Vec<ShapePoint> {
    let num = |v: &Option<String>| v.as_deref().filter(|x| !x.is_empty()).map(|x| x.parse::<f64>().unwrap());
    let mut out: Vec<ShapePoint> = feed
        .shapes
        .iter()
        .map(|r| {
            (
                r.shape_id.clone().unwrap(),
                r.shape_pt_sequence.as_deref().unwrap().parse().unwrap(),
                num(&r.shape_pt_lat).unwrap(),
                num(&r.shape_pt_lon).unwrap(),
                num(&r.shape_dist_traveled),
            )
        })
        .collect();
    out.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    out
}

fn trip_shapes(feed: &Feed) -> BTreeMap<String, Option<String>> {
    feed.trips
        .iter()
        .map(|t| (t.trip_id.clone().unwrap(), t.shape_id.clone().filter(|s| !s.is_empty())))
        .collect()
}

// ===================================================================
// The laws.
// ===================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Every shape is an alignment under the feed's id — including
    /// shapes no trip uses — with its points in sequence order and
    /// the distances the feed published (and none it did not).
    #[test]
    fn every_shape_is_an_alignment(spec in spec_strategy()) {
        let net = transit_codec::import(&build_feed(&spec)).network;
        let ids: BTreeSet<&str> = (0..spec.shapes.len()).map(|k| SHAPE_IDS[k]).collect();
        prop_assert_eq!(net.alignments.keys().map(String::as_str).collect::<BTreeSet<_>>(), ids);
        for (k, shape) in spec.shapes.iter().enumerate() {
            let alignment = &net.alignments[SHAPE_IDS[k]];
            let got: Vec<(f64, f64, Option<f64>)> =
                alignment.points.iter().map(|p| (p.lat, p.lon, p.distance)).collect();
            let want: Vec<(f64, f64, Option<f64>)> =
                shape.points.iter().enumerate().map(|(i, &(lat, lon))| (lat, lon, shape.distance(i))).collect();
            prop_assert_eq!(got, want, "shape {}", SHAPE_IDS[k]);
        }
    }

    /// A trip's shape is the alignment its pattern runs; a trip with
    /// no shape runs a pattern with none. One shape used by different
    /// stop lists is one alignment, run by each of their patterns.
    #[test]
    fn a_trip_runs_its_shape(spec in spec_strategy()) {
        let net = transit_codec::import(&build_feed(&spec)).network;
        for (i, t) in spec.trips.iter().enumerate() {
            let id = format!("t{i}");
            let (_, pt, _) = net.trip(&id).ok_or_else(|| TestCaseError::fail(format!("{id} missing")))?;
            prop_assert_eq!(
                net.patterns[&pt.pattern].alignment.as_deref(),
                t.shape.map(|k| SHAPE_IDS[k]),
                "{}",
                id
            );
        }
        prop_assert!(validation_clean(&net));
    }

    /// Export writes every alignment back as the feed published it —
    /// its id, its sequence numbers, its points, its distances — and
    /// every trip keeps its shape id.
    #[test]
    fn shapes_export_as_published(spec in spec_strategy()) {
        let feed = build_feed(&spec);
        let outcome = transit_codec::import(&feed);
        let out = transit_codec::export(&outcome.network, &outcome.boundary).map_err(TestCaseError::fail)?;
        prop_assert_eq!(shape_points(&out), shape_points(&feed));
        prop_assert_eq!(trip_shapes(&out), trip_shapes(&feed));
    }
}

fn validation_clean(net: &transit_network::Network) -> bool {
    transit_network::validation_issues(net).is_empty()
}
