//! The bus profile's laws (D-26, a bus): a planner draws a bus along
//! ANY class of street — the data's class is a grey area the planner
//! judges — and the classes the data does not clearly allow (unknown,
//! and the path-like path, steps and track) come back named, as
//! streets to confirm, never as a refusal.
//!
//! Written BEFORE the implementation, from the decision:
//!
//! 1. **Never refused** — a route's points and length do not depend on
//!    the classes of the streets it runs: the same document with every
//!    class relabeled answers the same path.
//! 2. **The streets it follows** — a route names the streets it runs,
//!    in order (`streets`: class, name, meters along it); their meters
//!    add up to the route's length, none is empty, and two in a row
//!    differ (a street is one entry however many segments it has).
//!    Each is a street of the document.
//! 3. **On one street** — a route that never leaves a way names that
//!    way alone.
//! 4. **The profile** — the bus profile asks the planner to confirm
//!    exactly unknown, path, steps and track; nothing else.
//!
//! The API: `StreetNetwork::route_along(from, to) -> Option<StreetRoute
//! { points, length_m, streets: Vec<StreetUse { class, name, length_m
//! }> }>`; `transit_streets::bus::{CONFIRM, confirms}`.

use proptest::prelude::*;
use transit_streets::{bus, StreetClass, StreetNetwork, Streets, Way};

const CLASSES: [StreetClass; 10] = [
    StreetClass::Motorway,
    StreetClass::Primary,
    StreetClass::Secondary,
    StreetClass::Local,
    StreetClass::Ramp,
    StreetClass::Service,
    StreetClass::Track,
    StreetClass::Path,
    StreetClass::Steps,
    StreetClass::Unknown,
];

const N: i32 = 4;
const STEP: i32 = 1_000; // microdegrees: ~110 m
const LAT0: i32 = 45_600_000;
const LON0: i32 = -122_600_000;

fn p(r: i32, c: i32) -> [i32; 2] {
    [LAT0 + r * STEP, LON0 + c * STEP]
}

#[derive(Clone, Debug)]
struct Spec {
    /// Per way (rows first, then columns): present, class, a name or
    /// none, one-way.
    ways: Vec<(bool, usize, Option<u8>, bool)>,
    from: (f64, f64),
    to: (f64, f64),
}

fn spec() -> impl Strategy<Value = Spec> {
    let way = (proptest::bool::weighted(0.85), 0..CLASSES.len(), proptest::option::of(0u8..3), proptest::bool::weighted(0.2));
    let at = (0.0f64..(N - 1) as f64, 0.0f64..(N - 1) as f64);
    (proptest::collection::vec(way, (2 * N) as usize), at.clone(), at)
        .prop_map(|(ways, from, to)| Spec { ways, from, to })
}

/// A lattice: N rows and N columns of straight streets, each split at
/// every crossing (ways weld where they share a point).
fn streets(spec: &Spec, relabel: Option<StreetClass>) -> Streets {
    let mut ways = Vec::new();
    for (i, &(present, class, name, oneway)) in spec.ways.iter().enumerate() {
        if !present {
            continue;
        }
        let i = i as i32;
        let points: Vec<[i32; 2]> =
            if i < N { (0..N).map(|c| p(i, c)).collect() } else { (0..N).map(|r| p(r, i - N)).collect() };
        ways.push(Way {
            points,
            oneway,
            class: relabel.unwrap_or(CLASSES[class]),
            name: name.map(|n| format!("Street {n}")),
        });
    }
    Streets { ways }
}

fn degrees((r, c): (f64, f64)) -> (f64, f64) {
    ((LAT0 as f64 + r * STEP as f64) / 1e6, (LON0 as f64 + c * STEP as f64) / 1e6)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_bus_is_drawn_whatever_the_class(spec in spec(), relabel in 0..CLASSES.len()) {
        let doc = streets(&spec, None);
        let plain = streets(&spec, Some(CLASSES[relabel]));
        let (g, h) = (StreetNetwork::build(&doc), StreetNetwork::build(&plain));
        let (fl, fo) = degrees(spec.from);
        let (tl, to) = degrees(spec.to);
        let (Some(from), Some(to)) = (g.snap(fl, fo), g.snap(tl, to)) else { return Ok(()) };
        let a = g.route_along(from, to);
        let b = h.route_along(from, to);
        prop_assert_eq!(a.as_ref().map(|r| (&r.points, r.length_m)), b.as_ref().map(|r| (&r.points, r.length_m)));
        // …and the same as the plain route the graph always answered.
        prop_assert_eq!(a.as_ref().map(|r| (r.points.clone(), r.length_m)), g.route_with_length(from, to));
    }

    #[test]
    fn a_route_names_the_streets_it_follows(spec in spec()) {
        let doc = streets(&spec, None);
        let g = StreetNetwork::build(&doc);
        let (fl, fo) = degrees(spec.from);
        let (tl, to) = degrees(spec.to);
        let (Some(from), Some(to)) = (g.snap(fl, fo), g.snap(tl, to)) else { return Ok(()) };
        let Some(route) = g.route_along(from, to) else { return Ok(()) };
        let total: f64 = route.streets.iter().map(|s| s.length_m).sum();
        prop_assert!((total - route.length_m).abs() <= 1e-6 * route.length_m.max(1.0), "{} vs {}", total, route.length_m);
        prop_assert!(route.streets.iter().all(|s| s.length_m > 0.0), "no empty entry");
        for pair in route.streets.windows(2) {
            prop_assert!((pair[0].class, &pair[0].name) != (pair[1].class, &pair[1].name), "one entry per street");
        }
        for s in &route.streets {
            prop_assert!(doc.ways.iter().any(|w| w.class == s.class && w.name == s.name), "{:?} is a street of the document", s);
        }
    }
}

#[test]
fn a_route_along_one_street_names_it_alone() {
    let doc = Streets {
        ways: vec![
            Way { points: vec![p(0, 0), p(0, 1), p(0, 2), p(0, 3)], oneway: false, class: StreetClass::Path, name: Some("Trail".into()) },
            Way { points: vec![p(1, 0), p(1, 3)], oneway: false, class: StreetClass::Primary, name: Some("Main".into()) },
        ],
    };
    let g = StreetNetwork::build(&doc);
    let from = g.snap(45.6, -122.5999).unwrap();
    let to = g.snap(45.6, -122.5972).unwrap();
    let route = g.route_along(from, to).expect("a bus may be drawn down a path");
    assert_eq!(route.streets.len(), 1);
    assert_eq!(route.streets[0].class, StreetClass::Path);
    assert_eq!(route.streets[0].name.as_deref(), Some("Trail"));
    assert!((route.streets[0].length_m - route.length_m).abs() < 1e-9);
}

#[test]
fn the_bus_profile_asks_to_confirm_the_grey_classes() {
    let confirmed: Vec<StreetClass> = CLASSES.into_iter().filter(|c| bus::confirms(*c)).collect();
    assert_eq!(confirmed, vec![StreetClass::Track, StreetClass::Path, StreetClass::Steps, StreetClass::Unknown]);
    let mut listed = bus::CONFIRM.to_vec();
    listed.sort_by_key(|c| c.as_str());
    let mut expected = confirmed.clone();
    expected.sort_by_key(|c| c.as_str());
    assert_eq!(listed, expected);
}
