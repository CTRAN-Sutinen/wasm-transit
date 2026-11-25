//! Pins the walk table and the walks from a point against an oracle that
//! rebuilds the walking graph with its own snap and Dijkstra
//! (`crates/cap-streets/OPS.md` "walks" and "access").
//!
//! The oracle accepts the walk from any of several equally near positions,
//! because the nearest street is not unique where two lines overlap without
//! joining.

use std::collections::{BTreeMap, BinaryHeap, HashMap};

use proptest::collection::vec;
use proptest::prelude::*;
use serde_json::json;
use transit_streets::geojson::import;
use transit_streets::walking::{walks, walks_from, Walk, MAX_WALK_SECS, WALK_SPEED_MPS};
use transit_streets::Streets;

const LAT0: f64 = 45.6;
const LON0: f64 = -122.6;
const STEP: f64 = 0.001;
const CELL: f64 = 0.001;

fn meters_between(a_lat: f64, a_lon: f64, b_lat: f64, b_lon: f64) -> f64 {
    let (p1, p2) = (a_lat.to_radians(), b_lat.to_radians());
    let (dp, dl) = ((b_lat - a_lat).to_radians(), (b_lon - a_lon).to_radians());
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6_371_000.0 * h.sqrt().min(1.0).asin()
}

const M_PER_DEG: f64 = 6_371_000.0 * std::f64::consts::PI / 180.0;

const CLASSES: &[&str] = &["A10", "A30", "A40", "A63", "A71", "A72", "X10", "A64"];

#[derive(Clone, Debug)]
struct Line {
    points: Vec<(i64, i64)>,
    cfcc: &'static str,
    oneway: bool,
}

fn line() -> impl Strategy<Value = Line> {
    (vec((0i64..7, 0i64..7), 2..=5), prop::sample::select(CLASSES), any::<bool>())
        .prop_map(|(points, cfcc, oneway)| Line { points, cfcc, oneway })
}

fn layer_bytes(lines: &[Line]) -> Vec<u8> {
    let features: Vec<_> = lines
        .iter()
        .map(|l| {
            let mut props = serde_json::Map::new();
            props.insert("CFCC".into(), json!(l.cfcc));
            if l.oneway {
                props.insert("OneWay".into(), json!(">"));
            }
            json!({ "type": "Feature", "properties": props, "geometry": {
                "type": "LineString",
                "coordinates": l.points.iter().map(|(a, b)| json!([LON0 + *b as f64 * STEP, LAT0 + *a as f64 * STEP])).collect::<Vec<_>>(),
            }})
        })
        .collect();
    serde_json::to_vec(&json!({ "type": "FeatureCollection", "features": features })).unwrap()
}

/// Fractional lattice positions are never exactly equidistant from two
/// streets, so the nearest street is unique.
fn stops() -> impl Strategy<Value = Vec<(String, f64, f64)>> {
    vec((-1.0f64..7.0, -1.0f64..7.0), 1..=7).prop_map(|pts| {
        pts.into_iter()
            .enumerate()
            .map(|(i, (a, b))| (format!("s{i}"), LAT0 + a * STEP, LON0 + b * STEP))
            .collect()
    })
}

fn point() -> impl Strategy<Value = (f64, f64)> {
    (-1.0f64..7.0, -1.0f64..7.0).prop_map(|(a, b)| (LAT0 + a * STEP, LON0 + b * STEP))
}

fn case() -> impl Strategy<Value = (Streets, Vec<(String, f64, f64)>, f64, u32)> {
    (vec(line(), 1..=7), stops(), 0.8f64..2.0, 30u32..900)
        .prop_filter_map("the layer has a street", |(lines, stops, speed, max)| {
            import(&layer_bytes(&lines)).ok().map(|doc| (doc, stops, speed, max))
        })
}

fn walkable(class: &str) -> bool {
    !matches!(class, "motorway" | "ramp" | "unknown")
}

struct OSeg {
    a: (f64, f64),
    b: (f64, f64),
    len: f64,
    na: usize,
    nb: usize,
}

struct Oracle {
    segs: Vec<OSeg>,
    adj: Vec<Vec<(usize, f64)>>,
}

impl Oracle {
    fn new(doc: &Streets) -> Oracle {
        let mut ids: HashMap<[i32; 2], usize> = HashMap::new();
        let mut segs = Vec::new();
        for way in &doc.ways {
            let class = serde_json::to_value(way.class).unwrap();
            if !walkable(class.as_str().unwrap()) {
                continue;
            }
            for pair in way.points.windows(2) {
                let n = ids.len();
                let na = *ids.entry(pair[0]).or_insert(n);
                let n = ids.len();
                let nb = *ids.entry(pair[1]).or_insert(n);
                let a = (pair[0][0] as f64 / 1e6, pair[0][1] as f64 / 1e6);
                let b = (pair[1][0] as f64 / 1e6, pair[1][1] as f64 / 1e6);
                segs.push(OSeg { a, b, len: meters_between(a.0, a.1, b.0, b.1), na, nb });
            }
        }
        let mut adj = vec![Vec::new(); ids.len()];
        for s in &segs {
            adj[s.na].push((s.nb, s.len));
            adj[s.nb].push((s.na, s.len));
        }
        Oracle { segs, adj }
    }

    /// The equally near positions as (segment, t, distance in meters).
    fn snap(&self, lat: f64, lon: f64) -> Vec<(usize, f64, f64)> {
        let (qx, qy) = ((lon / CELL).floor() as i64, (lat / CELL).floor() as i64);
        let m_lat = M_PER_DEG;
        let m_lon = M_PER_DEG * lat.to_radians().cos();
        let mut all: Vec<(usize, f64, f64)> = Vec::new();
        for (i, s) in self.segs.iter().enumerate() {
            let ix0 = (s.a.1.min(s.b.1) / CELL).floor() as i64;
            let ix1 = (s.a.1.max(s.b.1) / CELL).floor() as i64;
            let iy0 = (s.a.0.min(s.b.0) / CELL).floor() as i64;
            let iy1 = (s.a.0.max(s.b.0) / CELL).floor() as i64;
            if ix1 < qx - 1 || ix0 > qx + 1 || iy1 < qy - 1 || iy0 > qy + 1 {
                continue;
            }
            let (ax, ay) = ((s.a.1 - lon) * m_lon, (s.a.0 - lat) * m_lat);
            let (bx, by) = ((s.b.1 - lon) * m_lon, (s.b.0 - lat) * m_lat);
            let (dx, dy) = (bx - ax, by - ay);
            let t = (-(ax * dx + ay * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
            all.push((i, t, (ax + t * dx).hypot(ay + t * dy)));
        }
        let Some(min) = all.iter().map(|c| c.2).min_by(f64::total_cmp) else { return all };
        all.retain(|c| c.2 <= min + 1e-6);
        all
    }

    fn dijkstra(&self, sources: &[(usize, f64)]) -> Vec<f64> {
        let mut dist = vec![f64::INFINITY; self.adj.len()];
        let mut heap = BinaryHeap::new();
        for &(n, c) in sources {
            if c < dist[n] {
                dist[n] = c;
                heap.push(std::cmp::Reverse((c.to_bits(), n)));
            }
        }
        while let Some(std::cmp::Reverse((bits, u))) = heap.pop() {
            let d = f64::from_bits(bits);
            if d > dist[u] {
                continue;
            }
            for &(v, len) in &self.adj[u] {
                if d + len < dist[v] {
                    dist[v] = d + len;
                    heap.push(std::cmp::Reverse(((d + len).to_bits(), v)));
                }
            }
        }
        dist
    }

    fn between(&self, (sa, ta, _): (usize, f64, f64), (sb, tb, _): (usize, f64, f64)) -> f64 {
        let a = &self.segs[sa];
        let b = &self.segs[sb];
        let dist = self.dijkstra(&[(a.na, ta * a.len), (a.nb, (1.0 - ta) * a.len)]);
        let mut best = (dist[b.na] + tb * b.len).min(dist[b.nb] + (1.0 - tb) * b.len);
        if sa == sb {
            best = best.min((ta - tb).abs() * a.len);
        }
        best
    }

    /// Walk seconds for each choice of equally near positions. Infinite
    /// means no street path.
    fn walks(&self, stops: &[(String, f64, f64)], speed: f64) -> BTreeMap<(String, String), Vec<f64>> {
        let snapped: Vec<_> = stops.iter().map(|(_, lat, lon)| self.snap(*lat, *lon)).collect();
        let mut out = BTreeMap::new();
        for (i, a) in stops.iter().enumerate() {
            for (j, b) in stops.iter().enumerate() {
                if i == j || snapped[i].is_empty() || snapped[j].is_empty() {
                    continue;
                }
                let mut choices = Vec::new();
                for &pa in &snapped[i] {
                    for &pb in &snapped[j] {
                        choices.push(((pa.2 + self.between(pa, pb) + pb.2) / speed).ceil());
                    }
                }
                out.insert((a.0.clone(), b.0.clone()), choices);
            }
        }
        out
    }
}

fn as_tuples(got: &[Walk]) -> Vec<(String, String, u32)> {
    got.iter().map(|w| (w.from.clone(), w.to.clone(), w.walk_secs)).collect()
}

/// Agrees up to a second of rounding, because the two sum the same meters in
/// different orders and a walk near a whole second or the limit may round
/// either way.
fn agree(got: &[(String, String, u32)], want: &BTreeMap<(String, String), Vec<f64>>, max: u32) -> Result<(), String> {
    let g: HashMap<_, _> = got.iter().map(|(a, b, s)| ((a.clone(), b.clone()), *s as f64)).collect();
    let max = max as f64;
    for (k, s) in &g {
        let Some(choices) = want.get(k) else { return Err(format!("{k:?}: got {s} s, a stop the oracle snaps nowhere")) };
        if !choices.iter().any(|c| (c - s).abs() <= 1.0) {
            return Err(format!("{k:?}: got {s} s, the oracle's choices {choices:?}"));
        }
    }
    for (k, choices) in want {
        if !g.contains_key(k) && !choices.iter().any(|c| *c + 1.0 >= max) {
            return Err(format!("{k:?}: no walk, the oracle's choices {choices:?} (limit {max})"));
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Every walk is the oracle's, and none is past the limit.
    #[test]
    fn walks_match_the_oracle((doc, stops, speed, max) in case()) {
        let got = walks(&doc, &stops, speed, max);
        let want = Oracle::new(&doc).walks(&stops, speed);
        let got = as_tuples(&got);
        if let Err(e) = agree(&got, &want, max) {
            prop_assert!(false, "{e}");
        }
        prop_assert!(got.iter().all(|(_, _, s)| *s <= max), "no walk past the limit");
    }

    /// A walk is symmetric and never from a stop to itself. The table is in
    /// (from, to) order with no pair twice.
    #[test]
    fn walks_are_symmetric_and_ordered((doc, stops, speed, max) in case()) {
        let got = as_tuples(&walks(&doc, &stops, speed, max));
        let pairs: HashMap<_, _> = got.iter().map(|(a, b, s)| ((a.clone(), b.clone()), *s)).collect();
        for (a, b, s) in &got {
            prop_assert_ne!(a, b);
            prop_assert_eq!(pairs.get(&(b.clone(), a.clone())), Some(s), "symmetric");
        }
        let keys: Vec<_> = got.iter().map(|(a, b, _)| (a.clone(), b.clone())).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        sorted.dedup();
        prop_assert_eq!(keys, sorted);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// A point walks as a stop at the point would.
    #[test]
    fn a_point_walks_as_a_stop_there_walks((doc, stops, speed, max) in case(), (lat, lon) in point()) {
        let got: Vec<_> = walks_from(&doc, lat, lon, &stops, speed, max)
            .into_iter()
            .map(|w| ("@".to_string(), w.stop_id, w.walk_secs))
            .collect();
        let mut with_point = stops.clone();
        with_point.push(("@".into(), lat, lon));
        let want: BTreeMap<_, _> = Oracle::new(&doc)
            .walks(&with_point, speed)
            .into_iter()
            .filter(|((from, _), _)| from == "@")
            .collect();
        if let Err(e) = agree(&got, &want, max) {
            prop_assert!(false, "{e}");
        }
        prop_assert!(got.iter().all(|(_, _, s)| *s <= max), "no walk past the limit");
    }

    /// Each path starts at the point, runs along walkable streets, ends at
    /// the stop, and is as long as the walk.
    #[test]
    fn a_walk_from_a_point_draws_its_path((doc, stops, speed, max) in case(), (lat, lon) in point()) {
        let oracle = Oracle::new(&doc);
        for w in walks_from(&doc, lat, lon, &stops, speed, max) {
            let path = &w.path;
            let (_, s_lat, s_lon) = stops.iter().find(|s| s.0 == w.stop_id).unwrap();
            prop_assert!(path.len() >= 2);
            prop_assert_eq!((path[0].lat, path[0].lon), (lat, lon), "the path starts at the point");
            let last = path[path.len() - 1];
            prop_assert_eq!((last.lat, last.lon), (*s_lat, *s_lon), "the path ends at the stop");
            for p in &path[1..path.len() - 1] {
                let on = oracle.snap(p.lat, p.lon).first().is_some_and(|c| c.2 < 0.05);
                prop_assert!(on, "{p:?} is on no walkable street");
            }
            let meters: f64 = path.windows(2).map(|q| meters_between(q[0].lat, q[0].lon, q[1].lat, q[1].lon)).sum();
            let secs = meters / speed;
            prop_assert!(
                secs <= w.walk_secs as f64 + 0.01 && secs > w.walk_secs as f64 - 1.01,
                "the path takes {secs} s, the walk {} s", w.walk_secs
            );
        }
    }
}

fn street(cfcc: &str, oneway: bool, coords: &[(f64, f64)]) -> serde_json::Value {
    let mut props = serde_json::Map::new();
    props.insert("CFCC".into(), json!(cfcc));
    if oneway {
        props.insert("OneWay".into(), json!(">"));
    }
    json!({ "type": "Feature", "properties": props, "geometry": {
        "type": "LineString", "coordinates": coords.iter().map(|(lat, lon)| json!([lon, lat])).collect::<Vec<_>>() } })
}

fn doc(features: Vec<serde_json::Value>) -> Streets {
    import(&serde_json::to_vec(&json!({ "type": "FeatureCollection", "features": features })).unwrap()).unwrap()
}

fn stop(id: &str, lat: f64, lon: f64) -> (String, f64, f64) {
    (id.to_string(), lat, lon)
}

#[test]
fn a_rider_never_walks_a_motorway_a_ramp_or_an_unknown_street() {
    let stops = [stop("a", 45.6000, -122.6000), stop("b", 45.6000, -122.5950)];
    for cfcc in ["A10", "A63", "X10"] {
        let only = doc(vec![street(cfcc, false, &[(45.6, -122.6), (45.6, -122.595)])]);
        assert!(walks(&only, &stops, WALK_SPEED_MPS, MAX_WALK_SECS).is_empty(), "{cfcc} is not walkable");
    }
    let beside = doc(vec![
        street("A10", false, &[(45.6, -122.6), (45.6, -122.595)]),
        street("A40", false, &[(45.6002, -122.6), (45.6002, -122.595)]),
    ]);
    let got = walks(&beside, &stops, WALK_SPEED_MPS, MAX_WALK_SECS);
    assert_eq!(got.len(), 2, "a to b and b to a, along the local street");
    let meters = meters_between(45.6002, -122.6, 45.6002, -122.595) + 2.0 * 0.0002 * M_PER_DEG;
    assert!(got[0].walk_secs.abs_diff((meters / WALK_SPEED_MPS).ceil() as u32) <= 1, "{got:?}");
}

#[test]
fn a_rider_walks_a_one_way_street_both_ways() {
    let one_way = doc(vec![street("A40", true, &[(45.6, -122.6), (45.6, -122.597)])]);
    let stops = [stop("a", 45.6, -122.6), stop("b", 45.6, -122.597)];
    let got = walks(&one_way, &stops, WALK_SPEED_MPS, MAX_WALK_SECS);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].walk_secs, got[1].walk_secs);
}

#[test]
fn a_walk_past_the_limit_is_no_walk() {
    // About 1.2 km of street is 15 minutes at 1.3 m/s.
    let long = doc(vec![street("A40", false, &[(45.6, -122.6), (45.6, -122.585)])]);
    let stops = [stop("a", 45.6, -122.6), stop("b", 45.6, -122.585)];
    assert!(walks(&long, &stops, WALK_SPEED_MPS, MAX_WALK_SECS).is_empty());
    assert_eq!(walks(&long, &stops, WALK_SPEED_MPS, 1_200).len(), 2);
    assert_eq!(MAX_WALK_SECS, 600);
    assert_eq!(WALK_SPEED_MPS, 1.3);
}
