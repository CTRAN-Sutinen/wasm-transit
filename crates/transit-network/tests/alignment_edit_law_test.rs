//! The alignment-authoring laws (D-11): `draw_alignment` redraws a
//! pattern's line, and the edit NAMES what it redraws — `one` (the
//! pattern gets its own new alignment; the others on the old line keep
//! it) or `all` (the alignment itself changes, so every pattern on it
//! is redrawn) — and the change reports every pattern it touched.
//! Nothing moves silently.
//!
//! Written BEFORE the implementation, from the decision; every
//! expected answer is recomputed here from the generated input:
//!
//! 1. **One** — the pattern draws the new points under a new alignment
//!    (created, reported); every other pattern draws what it drew, and
//!    no existing alignment changes. Touched: the pattern alone.
//! 2. **All** — every pattern that ran the pattern's alignment draws
//!    the new points, under the same alignment id; every other pattern
//!    draws what it drew. Touched: exactly those patterns, in id order.
//!    A pattern with no alignment has no shared line: `all` gives it a
//!    new one, as `one` does.
//! 3. **Nothing silent** — every pattern whose line changed is in
//!    `touched`; the touched patterns' stop distances stop claiming
//!    to be published (they were measured along the old line), keep
//!    their meters, and nothing else in the document changes (stops,
//!    timing, trips, blocks, the other patterns).
//! 4. **Refusal** — an unknown pattern, fewer than two points, a point
//!    off the globe, or a taken alignment id is a named error; the
//!    input is untouched.
//!
//! The API these laws require: `NetworkEdit::DrawAlignment { pattern,
//! points: Vec<Coord>, redraw: Redraw (one | all), id: Option<String> }`
//! (`id` names the new alignment of `one`, minted `al{n}` when absent);
//! `apply_edits_reporting` answers `Applied { network, created,
//! touched }`, `touched: Vec<TouchedEntity { kind, id }>`.

use proptest::prelude::*;
use transit_network::{
    apply_edits, apply_edits_reporting, pattern_alignment, Alignment, AlignmentPoint, Coord, Mode, Network,
    NetworkEdit, Pattern, PatternStop, Redraw, Route, Stop,
};

const ALIGNMENT_IDS: [&str; 3] = ["sh 7", "al1", "x_line"];

#[derive(Clone, Debug)]
struct Spec {
    /// Points per alignment.
    alignments: Vec<Vec<(f64, f64)>>,
    /// Per pattern: the alignment it runs, and whether its stop
    /// distances were published.
    patterns: Vec<(Option<usize>, bool)>,
    /// The pattern drawn, and the new line.
    target: usize,
    points: Vec<(f64, f64)>,
}

fn coord() -> impl Strategy<Value = (f64, f64)> {
    (-800_000i32..800_000, -1_700_000i32..1_700_000).prop_map(|(a, b)| (a as f64 / 1e4, b as f64 / 1e4))
}

fn spec() -> impl Strategy<Value = Spec> {
    proptest::collection::vec(proptest::collection::vec(coord(), 2..=4), 0..=ALIGNMENT_IDS.len())
        .prop_flat_map(|alignments| {
            let n = alignments.len();
            let pattern = (
                if n == 0 { Just(None).boxed() } else { proptest::option::of(0..n).boxed() },
                any::<bool>(),
            );
            (Just(alignments), proptest::collection::vec(pattern, 1..=5))
        })
        .prop_flat_map(|(alignments, patterns)| {
            let k = patterns.len();
            (Just(alignments), Just(patterns), 0..k, proptest::collection::vec(coord(), 2..=6))
        })
        .prop_map(|(alignments, patterns, target, points)| Spec { alignments, patterns, target, points })
}

fn build(spec: &Spec) -> Network {
    let mut net = Network { name: "Generated".into(), ..Default::default() };
    for (i, lat) in [45.5, 45.51, 45.52].into_iter().enumerate() {
        net.stops.insert(format!("s{i}"), Stop { name: format!("Stop {i}"), lat, lon: -122.6, ..Default::default() });
    }
    net.routes.insert("r0".into(), Route { name: "R".into(), mode: Mode::Bus, ..Default::default() });
    for (i, points) in spec.alignments.iter().enumerate() {
        net.alignments.insert(
            ALIGNMENT_IDS[i].into(),
            Alignment {
                points: points
                    .iter()
                    .enumerate()
                    .map(|(k, &(lat, lon))| AlignmentPoint { lat, lon, distance: Some(k as f64 * 100.0) })
                    .collect(),
            },
        );
    }
    for (p, &(alignment, published)) in spec.patterns.iter().enumerate() {
        net.patterns.insert(
            format!("p{p}"),
            Pattern {
                route: "r0".into(),
                name: format!("Pattern {p}"),
                alignment: alignment.map(|a| ALIGNMENT_IDS[a].to_string()),
                stops: (0..3)
                    .map(|i| PatternStop {
                        stop: format!("s{i}"),
                        timepoint: i != 1,
                        distance_m: i as f64 * 400.0,
                        distance_published: published,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
        );
    }
    net
}

fn line(points: &[(f64, f64)]) -> Vec<Coord> {
    points.iter().map(|&(lat, lon)| Coord { lat, lon }).collect()
}

fn draw(spec: &Spec, redraw: Redraw) -> NetworkEdit {
    NetworkEdit::DrawAlignment { pattern: format!("p{}", spec.target), points: line(&spec.points), redraw, id: None }
}

/// A pattern with its line fields (alignment, published flags) blanked
/// — what the edit must leave alone.
fn rest(p: &Pattern) -> Pattern {
    let mut p = p.clone();
    p.alignment = None;
    for s in &mut p.stops {
        s.distance_published = false;
    }
    p
}

/// Law 3, shared by both scopes.
fn nothing_silent(net: &Network, next: &Network, touched: &[String]) -> Result<(), TestCaseError> {
    prop_assert_eq!(&next.stops, &net.stops);
    prop_assert_eq!(&next.routes, &net.routes);
    prop_assert_eq!(&next.pattern_trips, &net.pattern_trips);
    prop_assert_eq!(&next.blocks, &net.blocks);
    prop_assert_eq!(&next.service_types, &net.service_types);
    prop_assert_eq!(next.patterns.len(), net.patterns.len());
    for (id, before) in &net.patterns {
        let after = &next.patterns[id];
        prop_assert_eq!(rest(after), rest(before));
        let distances = |p: &Pattern| p.stops.iter().map(|s| s.distance_m).collect::<Vec<_>>();
        prop_assert_eq!(distances(after), distances(before));
        if touched.contains(id) {
            prop_assert!(after.stops.iter().all(|s| !s.distance_published), "{} still claims published distances", id);
        } else {
            prop_assert_eq!(after, before);
            prop_assert_eq!(pattern_alignment(next, id), pattern_alignment(net, id), "{} moved silently", id);
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn redrawing_one_pattern_gives_it_its_own_line(spec in spec()) {
        let net = build(&spec);
        let target = format!("p{}", spec.target);
        let applied = apply_edits_reporting(&net, &[draw(&spec, Redraw::One)]).expect("the redraw applies");
        let next = &applied.network;
        let new_id = next.patterns[&target].alignment.clone().expect("the pattern runs an alignment");
        prop_assert!(!net.alignments.contains_key(&new_id), "a NEW alignment");
        prop_assert_eq!(pattern_alignment(next, &target), line(&spec.points));
        prop_assert!(next.alignments[&new_id].points.iter().all(|p| p.distance.is_none()), "no distances were published along a drawn line");
        // Every alignment that stood still stands, unchanged.
        for (id, a) in &net.alignments {
            prop_assert_eq!(&next.alignments[id], a);
        }
        prop_assert_eq!(next.alignments.len(), net.alignments.len() + 1);
        let created: Vec<(String, String)> = applied.created.iter().map(|c| (c.kind.clone(), c.id.clone())).collect();
        prop_assert_eq!(created, vec![("alignment".to_string(), new_id.clone())]);
        let touched: Vec<String> = applied.touched.iter().map(|t| { assert_eq!(t.kind, "pattern"); t.id.clone() }).collect();
        prop_assert_eq!(&touched, &vec![target.clone()]);
        nothing_silent(&net, next, &touched)?;
    }

    #[test]
    fn redrawing_all_moves_every_pattern_on_the_line(spec in spec()) {
        let net = build(&spec);
        let target = format!("p{}", spec.target);
        let applied = apply_edits_reporting(&net, &[draw(&spec, Redraw::All)]).expect("the redraw applies");
        let next = &applied.network;
        let old = net.patterns[&target].alignment.clone();
        let sharing: Vec<String> = match &old {
            Some(a) => net.patterns.iter().filter(|(_, p)| p.alignment.as_ref() == Some(a)).map(|(id, _)| id.clone()).collect(),
            None => vec![target.clone()],
        };
        for id in &sharing {
            prop_assert_eq!(pattern_alignment(next, id), line(&spec.points), "{} runs the redrawn line", id);
        }
        match &old {
            Some(a) => {
                // The same alignment, redrawn: no id changes anywhere.
                prop_assert_eq!(next.alignments.len(), net.alignments.len());
                for (id, p) in &net.patterns {
                    prop_assert_eq!(&next.patterns[id].alignment, &p.alignment);
                }
                for (id, al) in &net.alignments {
                    if id != a {
                        prop_assert_eq!(&next.alignments[id], al);
                    }
                }
                prop_assert!(applied.created.is_empty());
            }
            None => {
                prop_assert_eq!(next.alignments.len(), net.alignments.len() + 1);
                prop_assert_eq!(applied.created.len(), 1);
                prop_assert_eq!(&applied.created[0].kind, "alignment");
            }
        }
        let touched: Vec<String> = applied.touched.iter().map(|t| t.id.clone()).collect();
        prop_assert_eq!(&touched, &sharing, "touched: exactly the patterns on the line, in id order");
        nothing_silent(&net, next, &touched)?;
    }

    #[test]
    fn a_redraw_that_cannot_apply_is_named(spec in spec(), bad in 0usize..4) {
        let net = build(&spec);
        let target = format!("p{}", spec.target);
        let edit = match bad {
            0 => NetworkEdit::DrawAlignment { pattern: "ghost".into(), points: line(&spec.points), redraw: Redraw::One, id: None },
            1 => NetworkEdit::DrawAlignment { pattern: target, points: line(&spec.points[..1]), redraw: Redraw::All, id: None },
            2 => NetworkEdit::DrawAlignment {
                pattern: target,
                points: vec![Coord { lat: 91.0, lon: 0.0 }, Coord { lat: 45.0, lon: f64::NAN }],
                redraw: Redraw::One,
                id: None,
            },
            _ => {
                let mut net = net.clone();
                net.alignments.insert("taken".into(), Alignment::default());
                let edit = NetworkEdit::DrawAlignment { pattern: target, points: line(&spec.points), redraw: Redraw::One, id: Some("taken".into()) };
                let err = apply_edits(&net, &[edit]).expect_err("a taken id is refused");
                prop_assert!(err.reason.contains("already exists"), "{}", err.reason);
                return Ok(());
            }
        };
        let err = apply_edits(&net, &[edit]).expect_err("refused");
        prop_assert_eq!(err.index, 0);
        prop_assert!(!err.reason.is_empty());
    }
}

#[test]
fn the_new_line_takes_the_callers_id_or_the_first_free_one() {
    let spec = Spec {
        alignments: vec![vec![(45.0, -122.0), (45.1, -122.1)], vec![(45.0, -122.0), (45.2, -122.2)]],
        patterns: vec![(Some(0), true), (Some(0), false)],
        target: 0,
        points: vec![(45.0, -122.0), (45.05, -122.02), (45.1, -122.1)],
    };
    let net = build(&spec);
    // "al1" is taken in the fixture: the mint skips it.
    let minted = apply_edits_reporting(&net, &[draw(&spec, Redraw::One)]).unwrap();
    assert_eq!(minted.network.patterns["p0"].alignment.as_deref(), Some("al2"));
    let named = apply_edits(
        &net,
        &[NetworkEdit::DrawAlignment { pattern: "p0".into(), points: line(&spec.points), redraw: Redraw::One, id: Some("p0-fix".into()) }],
    )
    .unwrap();
    assert_eq!(named.patterns["p0"].alignment.as_deref(), Some("p0-fix"));
    assert_eq!(named.patterns["p1"].alignment.as_deref(), Some("sh 7"), "the other pattern keeps the old line");
}

#[test]
fn the_edit_reads_from_json() {
    let edit: NetworkEdit = serde_json::from_str(
        r#"{"edit":"draw_alignment","pattern":"p0","redraw":"all","points":[{"lat":45.0,"lon":-122.0},{"lat":45.1,"lon":-122.1}]}"#,
    )
    .unwrap();
    assert!(matches!(edit, NetworkEdit::DrawAlignment { redraw: Redraw::All, id: None, .. }));
    // The scope is named, never defaulted: nothing moves silently.
    assert!(serde_json::from_str::<NetworkEdit>(
        r#"{"edit":"draw_alignment","pattern":"p0","points":[{"lat":45.0,"lon":-122.0},{"lat":45.1,"lon":-122.1}]}"#
    )
    .is_err());
}
