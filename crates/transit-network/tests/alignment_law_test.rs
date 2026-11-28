//! An alignment is its own record. Patterns reference it by id, and any number
//! of patterns, or none, can share one (docs/FORMATS.md, "Versions"). The
//! expected answers are recomputed from the generated input.

use proptest::prelude::*;
use serde_json::Value;
use transit_network::{
    migrate_network, pattern_alignment, validation_issues, Alignment, AlignmentPoint, Coord, Mode,
    Network, NetworkError, Pattern, PatternStop, Route, Stop,
};

#[derive(Clone, Debug)]
struct GenAlignment {
    points: Vec<(f64, f64)>,
    with_distances: bool,
}

#[derive(Clone, Debug)]
struct Spec {
    alignments: Vec<GenAlignment>,
    patterns: Vec<(Vec<usize>, Option<usize>)>,
    stops: Vec<(f64, f64)>,
}

const ALIGNMENT_IDS: [&str; 4] = ["sh 7", "A/1", "0", "x_line"];

fn coord() -> impl Strategy<Value = (f64, f64)> {
    (-900_000i32..900_000, -1_800_000i32..1_800_000)
        .prop_map(|(a, b)| (a as f64 / 1e4 + 0.123456789012, b as f64 / 1e4))
}

fn spec_strategy() -> impl Strategy<Value = Spec> {
    let alignment = (proptest::collection::vec(coord(), 0..=5), any::<bool>()).prop_map(
        |(points, with_distances)| GenAlignment {
            points,
            with_distances,
        },
    );
    (
        proptest::collection::vec(alignment, 0..=ALIGNMENT_IDS.len()),
        proptest::collection::vec(
            proptest::sample::select(vec![(45.5, -122.6), (45.51, -122.6), (45.52, -122.61)]),
            4,
        ),
    )
        .prop_flat_map(|(alignments, stops)| {
            let n = alignments.len();
            let pattern = (
                proptest::collection::vec(0usize..4, 1..=4),
                if n == 0 {
                    Just(None).boxed()
                } else {
                    proptest::option::of(0..n).boxed()
                },
            );
            (
                Just(alignments),
                proptest::collection::vec(pattern, 1..=4),
                Just(stops),
            )
        })
        .prop_map(|(alignments, patterns, stops)| Spec {
            alignments,
            patterns,
            stops,
        })
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
        extra: Default::default(),
    }
}

fn build(spec: &Spec) -> Network {
    let mut net = Network {
        name: "Generated".into(),
        ..Default::default()
    };
    for (i, &(lat, lon)) in spec.stops.iter().enumerate() {
        net.stops.insert(
            format!("s{i}"),
            Stop {
                name: format!("Stop {i}"),
                lat,
                lon,
                ..Default::default()
            },
        );
    }
    net.routes.insert(
        "r0".into(),
        Route {
            name: "R".into(),
            mode: Mode::Bus,
            ..Default::default()
        },
    );
    for (i, g) in spec.alignments.iter().enumerate() {
        net.alignments
            .insert(ALIGNMENT_IDS[i].into(), alignment_of(g));
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
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
        );
    }
    net
}

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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

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

    #[test]
    fn shared_and_unused_alignments_are_valid(spec in spec_strategy()) {
        let issues = validation_issues(&build(&spec));
        prop_assert!(
            !issues.iter().any(|i| matches!(i, NetworkError::UnknownAlignment { .. })),
            "{:?}",
            issues
        );
    }

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

    #[test]
    fn alignments_survive_the_document(spec in spec_strategy()) {
        let net = build(&spec);
        let text = serde_json::to_string(&net).unwrap();
        prop_assert_eq!(serde_json::from_str::<Network>(&text).unwrap(), net);
    }

    /// A v1 document with an inline `path` per pattern reads as an alignment
    /// under the pattern's id, drawn as before.
    #[test]
    fn a_pre_alignment_document_migrates(spec in spec_strategy()) {
        let net = build(&spec);
        let mut doc: Value = serde_json::to_value(&net).unwrap();
        let root = doc.as_object_mut().unwrap();
        root.remove("alignments");
        root.remove("kind");
        root.remove("schema_version");
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
        let migrated = migrate_network(doc).unwrap().0;
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
