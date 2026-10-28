//! The alignment laws: a line on the ground is its own record — its
//! own id, its points, the distances published along it — and a
//! pattern runs one by reference (D-4: the pattern points at the
//! alignment, so a pattern is its stops along its line). Alignments
//! are shared by any number of patterns, or by none.
//!
//! Written BEFORE the implementation, as the definition of the
//! behavior; every expected answer is recomputed here from the
//! generated input, with no code shared with the implementation.
//!
//! The API these laws require:
//!
//! * `Network.alignments: BTreeMap<AlignmentId, Alignment>`;
//!   `Alignment { points: Vec<AlignmentPoint> }` (`Default`);
//!   `AlignmentPoint { lat, lon, distance: Option<f64> }`.
//! * `Pattern.alignment: Option<AlignmentId>` — no inline path.
//! * `NetworkError::UnknownAlignment { pattern, alignment }`.
//! * `pattern_alignment` draws the referenced alignment (two points
//!   or more), else the stops joined.
//! * `read_network` reads a pre-alignment document (an inline
//!   `path` per pattern) as an alignment under the pattern's id —
//!   the id the old export gave its shape.

use proptest::prelude::*;
use serde_json::Value;
use transit_network::{
    pattern_alignment, read_network, validation_issues, Alignment, AlignmentPoint, Coord, Mode, Network,
    NetworkError, Pattern, PatternStop, Route, Stop,
};

// ===================================================================
// Generator: valid by construction.
// ===================================================================

/// One generated alignment: its points (lat, lon) and whether the
/// distances along it were published.
#[derive(Clone, Debug)]
struct GenAlignment {
    points: Vec<(f64, f64)>,
    with_distances: bool,
}

#[derive(Clone, Debug)]
struct Spec {
    /// Ids chosen to look nothing like pattern ids: an alignment's id
    /// is its own, never derived.
    alignments: Vec<GenAlignment>,
    /// Each pattern: its stop indices and which alignment it runs.
    patterns: Vec<(Vec<usize>, Option<usize>)>,
    /// Stop coordinates; repeats make consecutive duplicate points.
    stops: Vec<(f64, f64)>,
}

const ALIGNMENT_IDS: [&str; 4] = ["sh 7", "A/1", "0", "x_line"];

fn coord() -> impl Strategy<Value = (f64, f64)> {
    (-900_000i32..900_000, -1_800_000i32..1_800_000).prop_map(|(a, b)| (a as f64 / 1e4 + 0.123456789012, b as f64 / 1e4))
}

fn spec_strategy() -> impl Strategy<Value = Spec> {
    let alignment = (proptest::collection::vec(coord(), 0..=5), any::<bool>())
        .prop_map(|(points, with_distances)| GenAlignment { points, with_distances });
    (
        proptest::collection::vec(alignment, 0..=ALIGNMENT_IDS.len()),
        proptest::collection::vec(proptest::sample::select(vec![(45.5, -122.6), (45.51, -122.6), (45.52, -122.61)]), 4),
    )
        .prop_flat_map(|(alignments, stops)| {
            let n = alignments.len();
            let pattern = (
                proptest::collection::vec(0usize..4, 1..=4),
                if n == 0 { Just(None).boxed() } else { proptest::option::of(0..n).boxed() },
            );
            (Just(alignments), proptest::collection::vec(pattern, 1..=4), Just(stops))
        })
        .prop_map(|(alignments, patterns, stops)| Spec { alignments, patterns, stops })
}

fn alignment_of(g: &GenAlignment) -> Alignment {
    Alignment {
        points: g
            .points
            .iter()
            .enumerate()
            .map(|(i, &(lat, lon))| AlignmentPoint {
                lat,
                lon,
                distance: g.with_distances.then_some(i as f64 * 123.25),
            })
            .collect(),
    }
}

fn build(spec: &Spec) -> Network {
    let mut net = Network { name: "Generated".into(), ..Default::default() };
    for (i, &(lat, lon)) in spec.stops.iter().enumerate() {
        net.stops.insert(format!("s{i}"), Stop { name: format!("Stop {i}"), lat, lon, ..Default::default() });
    }
    net.routes.insert("r0".into(), Route { name: "R".into(), mode: Mode::Bus, ..Default::default() });
    for (i, g) in spec.alignments.iter().enumerate() {
        net.alignments.insert(ALIGNMENT_IDS[i].into(), alignment_of(g));
    }
    for (p, (stops, alignment)) in spec.patterns.iter().enumerate() {
        let n = stops.len();
        net.patterns.insert(
            format!("p{p}"),
            Pattern {
                route: "r0".into(),
                name: format!("Pattern {p}"),
                alignment: alignment.map(|a| ALIGNMENT_IDS[a].to_string()),
                stops: stops
                    .iter()
                    .enumerate()
                    .map(|(i, s)| PatternStop {
                        stop: format!("s{s}"),
                        timepoint: i == 0 || i == n - 1,
                        distance_m: i as f64 * 400.0,
                    })
                    .collect(),
                ..Default::default()
            },
        );
    }
    net
}

/// The stops joined in order, consecutive repeats dropped, empty
/// below two points — recomputed from the spec.
fn stops_joined(spec: &Spec, stops: &[usize]) -> Vec<Coord> {
    let mut out: Vec<Coord> = Vec::new();
    for &s in stops {
        let (lat, lon) = spec.stops[s];
        if out.last().is_some_and(|c| c.lat == lat && c.lon == lon) {
            continue;
        }
        out.push(Coord { lat, lon });
    }
    if out.len() < 2 {
        out.clear();
    }
    out
}

// ===================================================================
// The laws.
// ===================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// A pattern draws the alignment it runs — when that alignment is
    /// a line (two points or more) — else its stops joined.
    #[test]
    fn a_pattern_draws_its_alignment(spec in spec_strategy()) {
        let net = build(&spec);
        for (p, (stops, alignment)) in spec.patterns.iter().enumerate() {
            let line = alignment.map(|a| &spec.alignments[a]).filter(|g| g.points.len() >= 2);
            let expected = match line {
                Some(g) => g.points.iter().map(|&(lat, lon)| Coord { lat, lon }).collect(),
                None => stops_joined(&spec, stops),
            };
            prop_assert_eq!(pattern_alignment(&net, &format!("p{p}")), expected, "p{}", p);
        }
    }

    /// Alignments shared by several patterns, and alignments no
    /// pattern runs, are a valid network: an alignment stands on its
    /// own.
    #[test]
    fn shared_and_unused_alignments_are_valid(spec in spec_strategy()) {
        let issues = validation_issues(&build(&spec));
        prop_assert!(
            !issues.iter().any(|i| matches!(i, NetworkError::UnknownAlignment { .. })),
            "{:?}",
            issues
        );
    }

    /// A pattern that names an alignment the network lacks is a
    /// defect, named by pattern and alignment.
    #[test]
    fn an_unknown_alignment_is_a_defect(spec in spec_strategy(), which in any::<prop::sample::Index>()) {
        let mut net = build(&spec);
        let p = format!("p{}", which.index(spec.patterns.len()));
        net.patterns.get_mut(&p).unwrap().alignment = Some("no such line".into());
        let issues = validation_issues(&net);
        prop_assert!(
            issues.iter().any(|i| matches!(
                i,
                NetworkError::UnknownAlignment { pattern, alignment } if *pattern == p && alignment == "no such line"
            )),
            "{:?}",
            issues
        );
    }

    /// The document carries alignments exactly: every point, every
    /// published distance, every id, read back as written.
    #[test]
    fn alignments_survive_the_document(spec in spec_strategy()) {
        let net = build(&spec);
        let text = serde_json::to_string(&net).unwrap();
        prop_assert_eq!(read_network(&text).unwrap(), net);
    }

    /// A pre-alignment document — each pattern's line inline as its
    /// `path` — reads as an alignment under the pattern's id, run by
    /// that pattern, drawn exactly as before.
    #[test]
    fn a_pre_alignment_document_migrates(spec in spec_strategy()) {
        let net = build(&spec);
        let mut doc: Value = serde_json::to_value(&net).unwrap();
        let root = doc.as_object_mut().unwrap();
        root.remove("alignments");
        for (p, pattern) in root["patterns"].as_object_mut().unwrap().iter_mut() {
            let pattern = pattern.as_object_mut().unwrap();
            pattern.remove("alignment");
            let i: usize = p[1..].parse().unwrap();
            if let Some(a) = spec.patterns[i].1 {
                let path: Vec<Value> = spec.alignments[a]
                    .points
                    .iter()
                    .map(|&(lat, lon)| serde_json::json!({"lat": lat, "lon": lon}))
                    .collect();
                pattern.insert("path".into(), Value::Array(path));
            }
        }
        let migrated = read_network(&doc.to_string()).unwrap();
        for (i, (_, alignment)) in spec.patterns.iter().enumerate() {
            let p = format!("p{i}");
            prop_assert_eq!(pattern_alignment(&migrated, &p), pattern_alignment(&net, &p), "{} draws as before", p);
            match alignment {
                Some(a) => {
                    prop_assert_eq!(migrated.patterns[&p].alignment.as_deref(), Some(p.as_str()));
                    let points: Vec<(f64, f64)> =
                        migrated.alignments[&p].points.iter().map(|pt| (pt.lat, pt.lon)).collect();
                    prop_assert_eq!(&points, &spec.alignments[*a].points);
                    prop_assert!(migrated.alignments[&p].points.iter().all(|pt| pt.distance.is_none()));
                }
                None => prop_assert_eq!(migrated.patterns[&p].alignment.as_deref(), None),
            }
        }
        let referenced: Vec<String> = (0..spec.patterns.len())
            .filter(|&i| spec.patterns[i].1.is_some())
            .map(|i| format!("p{i}"))
            .collect();
        prop_assert_eq!(migrated.alignments.keys().cloned().collect::<Vec<_>>(), referenced);
    }
}
