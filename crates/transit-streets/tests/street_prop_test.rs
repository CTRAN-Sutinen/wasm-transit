//! Street network laws, as property tests with independent oracles.
//!
//! `streets_test.rs` covers hand-picked configurations. These laws
//! hold for ANY network the generator can produce:
//!
//! * **Structure** — the parsed graph's segment and node counts match
//!   the spec (welding by 1e-6 quantization, degenerate rows dropped).
//! * **Snap projection** — the snapped point lies on its segment,
//!   `dist_m` is the true distance to it, and re-snapping is a fixed
//!   point.
//! * **Snap nearest** — `dist_m` matches the brute-force nearest
//!   projection over every segment whose cell range intersects the
//!   query's 3x3 window; no street in the window means None.
//! * **Route optimality** — the routed path's total cost (the walk-in
//!   from the `from` position to its exit node included) equals the
//!   oracle's own Dijkstra over its own graph, one-way conventions
//!   included.
//! * **Reachability** — route returns None exactly when the oracle
//!   says unreachable, and a returned path ends exactly at `to`.
//! * **Totality** — arbitrary bytes never panic the parser.
//!
//! The oracle rebuilds the graph from the generated spec with its own
//! code — same documented rules, none of the implementation's code.

use proptest::collection::vec;
use proptest::prelude::*;
use std::collections::{BinaryHeap, HashMap};
use transit_streets::{StreetNetwork, StreetPosition};

const CELL: f64 = 0.001;
const LAT0: f64 = 45.0;
const LON0: f64 = -73.0;

/// Equirectangular meters — the module's documented metric, written
/// out independently here.
fn meters_between(a_lat: f64, a_lon: f64, b_lat: f64, b_lon: f64) -> f64 {
    let cos_lat = ((a_lat + b_lat) / 2.0).to_radians().cos();
    let dy = (b_lat - a_lat) * 110_540.0;
    let dx = (b_lon - a_lon) * 111_320.0 * cos_lat;
    dx.hypot(dy)
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

// ---------------------------------------------------------------------
// Generator
// ---------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpecDir {
    Both,
    Forward,
    Backward,
}

#[derive(Clone, Debug)]
struct Spec {
    /// One GeoJSON feature per entry; each line is a list of lattice
    /// vertices (lat units, lon units) of 0.001 deg.
    features: Vec<(SpecDir, Vec<Vec<(i64, i64)>>)>,
}

fn spec() -> impl Strategy<Value = Spec> {
    let dir = 0u8..3;
    (
        1usize..=6,                       // features
        vec(dir, 1..=6),                  // per-feature direction
        vec(vec(vec((0i64..8, 0i64..8), 2..=5), 1..=3), 1..=6), // lines/verts
    )
        .prop_map(|(n, dirs, feats)| Spec {
            features: feats
                .into_iter()
                .take(n)
                .zip(dirs)
                .map(|(lines, d)| {
                    let dir = match d {
                        0 => SpecDir::Both,
                        1 => SpecDir::Forward,
                        _ => SpecDir::Backward,
                    };
                    (dir, lines)
                })
                .collect(),
        })
}

fn vertex_lat(v: i64) -> f64 {
    LAT0 + v as f64 * CELL
}

fn vertex_lon(v: i64) -> f64 {
    LON0 + v as f64 * CELL
}

fn build_geojson(spec: &Spec) -> String {
    let mut features = Vec::new();
    for (dir, lines) in &spec.features {
        let oneway = match dir {
            SpecDir::Both => String::new(),
            SpecDir::Forward => "\"OneWay\":\">\"".into(),
            SpecDir::Backward => "\"OneWay\":\"<\"".into(),
        };
        let geometry = if lines.len() == 1 {
            let coords = lines[0]
                .iter()
                .map(|(lat, lon)| format!("[{},{}]", vertex_lon(*lon), vertex_lat(*lat)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{\"type\":\"LineString\",\"coordinates\":[{coords}]}}")
        } else {
            let all = lines
                .iter()
                .map(|line| {
                    format!(
                        "[{}]",
                        line.iter()
                            .map(|(lat, lon)| format!("[{},{}]", vertex_lon(*lon), vertex_lat(*lat)))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{\"type\":\"MultiLineString\",\"coordinates\":[{all}]}}")
        };
        features.push(format!(
            "{{\"type\":\"Feature\",\"properties\":{{{oneway}}},\"geometry\":{geometry}}}"
        ));
    }
    format!("{{\"type\":\"FeatureCollection\",\"features\":[{}]}}", features.join(","))
}

// ---------------------------------------------------------------------
// Oracle graph: rebuilt from the spec, no implementation code
// ---------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct OSeg {
    a: (f64, f64), // (lat, lon)
    b: (f64, f64),
    len: f64,
    dir: SpecDir,
    node_a: usize,
    node_b: usize,
}

#[derive(Default)]
struct OGraph {
    segs: Vec<OSeg>,
    nodes: Vec<(f64, f64)>,
    /// node -> [(next_node, length)]
    adj: Vec<Vec<(usize, f64)>>,
}

fn build_oracle(spec: &Spec) -> OGraph {
    let mut g = OGraph::default();
    let mut node_ids: HashMap<(i64, i64), usize> = HashMap::new();
    let node = |lat: f64, lon: f64, ids: &mut HashMap<(i64, i64), usize>, nodes: &mut Vec<(f64, f64)>| -> usize {
        let key = ((lat * 1e6).round() as i64, (lon * 1e6).round() as i64);
        *ids.entry(key).or_insert_with(|| {
            nodes.push((lat, lon));
            nodes.len() - 1
        })
    };
    for (dir, lines) in &spec.features {
        for line in lines {
            for pair in line.windows(2) {
                let a_lat = vertex_lat(pair[0].0);
                let a_lon = vertex_lon(pair[0].1);
                let b_lat = vertex_lat(pair[1].0);
                let b_lon = vertex_lon(pair[1].1);
                let len = meters_between(a_lat, a_lon, b_lat, b_lon);
                if len <= 0.0 {
                    continue; // degenerate rows are dropped, not welded
                }
                let node_a = node(a_lat, a_lon, &mut node_ids, &mut g.nodes);
                let node_b = node(b_lat, b_lon, &mut node_ids, &mut g.nodes);
                g.segs.push(OSeg {
                    a: (a_lat, a_lon),
                    b: (b_lat, b_lon),
                    len,
                    dir: *dir,
                    node_a,
                    node_b,
                });
            }
        }
    }
    g.adj = vec![Vec::new(); g.nodes.len()];
    for seg in &g.segs {
        if seg.dir != SpecDir::Backward {
            g.adj[seg.node_a].push((seg.node_b, seg.len));
        }
        if seg.dir != SpecDir::Forward {
            g.adj[seg.node_b].push((seg.node_a, seg.len));
        }
    }
    g
}

impl OGraph {
    fn allows_forward(&self, seg: usize) -> bool {
        self.segs[seg].dir != SpecDir::Backward
    }
    fn allows_backward(&self, seg: usize) -> bool {
        self.segs[seg].dir != SpecDir::Forward
    }

    /// Brute-force nearest projection over every segment whose cell
    /// range intersects the query's 3x3 cell window.
    fn snap_distance(&self, lat: f64, lon: f64) -> Option<(f64, f64, f64)> {
        let qx = (lon / CELL).floor() as i64;
        let qy = (lat / CELL).floor() as i64;
        let m_lat = 110_540.0;
        let m_lon = 111_320.0 * lat.to_radians().cos();
        let mut best: Option<(f64, f64, f64)> = None; // (plat, plon, dist)
        for seg in &self.segs {
            let ix0 = (seg.a.1.min(seg.b.1) / CELL).floor() as i64;
            let ix1 = (seg.a.1.max(seg.b.1) / CELL).floor() as i64;
            let iy0 = (seg.a.0.min(seg.b.0) / CELL).floor() as i64;
            let iy1 = (seg.a.0.max(seg.b.0) / CELL).floor() as i64;
            if ix1 < qx - 1 || ix0 > qx + 1 || iy1 < qy - 1 || iy0 > qy + 1 {
                continue; // not visible through any window cell
            }
            let ax = (seg.a.1 - lon) * m_lon;
            let ay = (seg.a.0 - lat) * m_lat;
            let bx = (seg.b.1 - lon) * m_lon;
            let by = (seg.b.0 - lat) * m_lat;
            let dx = bx - ax;
            let dy = by - ay;
            let len2 = dx * dx + dy * dy;
            let t = if len2 == 0.0 {
                0.0
            } else {
                (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0)
            };
            let px = ax + t * dx;
            let py = ay + t * dy;
            let dist = px.hypot(py);
            if best.is_none_or(|(_, _, d)| dist < d) {
                best = Some((lat + py / m_lat, lon + px / m_lon, dist));
            }
        }
        best
    }

    /// True point-segment distance (for the projection law), in meters.
    fn distance_to_segment(&self, lat: f64, lon: f64, seg: usize) -> f64 {
        let seg = &self.segs[seg];
        let m_lat = 110_540.0;
        let m_lon = 111_320.0 * lat.to_radians().cos();
        let ax = (seg.a.1 - lon) * m_lon;
        let ay = (seg.a.0 - lat) * m_lat;
        let bx = (seg.b.1 - lon) * m_lon;
        let by = (seg.b.0 - lat) * m_lat;
        let dx = bx - ax;
        let dy = by - ay;
        let len2 = dx * dx + dy * dy;
        let t = if len2 == 0.0 {
            0.0
        } else {
            (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0)
        };
        (ax + t * dx).hypot(ay + t * dy)
    }

    /// Dijkstra distances from the from-position's reachable segment
    /// ends (walk-in costs included).
    fn dijkstra_from(&self, from: StreetPosition) -> Vec<f64> {
        let seg = &self.segs[from.seg as usize];
        let mut starts: Vec<(f64, usize)> = Vec::new();
        if self.allows_forward(from.seg as usize) {
            starts.push(((1.0 - from.t) * seg.len, seg.node_b));
        }
        if self.allows_backward(from.seg as usize) {
            starts.push((from.t * seg.len, seg.node_a));
        }
        let n = self.nodes.len();
        let mut dist = vec![f64::INFINITY; n];
        let mut heap: BinaryHeap<std::cmp::Reverse<(u64, usize)>> = BinaryHeap::new();
        for (cost, node) in starts {
            if cost < dist[node] {
                dist[node] = cost;
                heap.push(std::cmp::Reverse((cost.to_bits(), node)));
            }
        }
        while let Some(std::cmp::Reverse((bits, u))) = heap.pop() {
            let d = f64::from_bits(bits);
            if d > dist[u] {
                continue;
            }
            for &(v, len) in &self.adj[u] {
                let nd = d + len;
                if nd < dist[v] {
                    dist[v] = nd;
                    heap.push(std::cmp::Reverse((nd.to_bits(), v)));
                }
            }
        }
        dist
    }

    /// The optimal cost from `from` to `to` in meters, or None when
    /// no street path exists.
    fn shortest_cost(&self, from: StreetPosition, to: StreetPosition) -> Option<f64> {
        let from_seg = from.seg as usize;
        let to_seg = &self.segs[to.seg as usize];

        let mut best = f64::INFINITY;
        // The trivial same-segment walk, when the direction allows it.
        if from.seg == to.seg {
            let reachable = match self.segs[from_seg].dir {
                SpecDir::Both => true,
                SpecDir::Forward => to.t >= from.t,
                SpecDir::Backward => to.t <= from.t,
            };
            if reachable {
                best = best.min((to.t - from.t).abs() * to_seg.len);
            }
        }
        // Entering the target segment at either end, when the
        // direction allows it.
        let dist = self.dijkstra_from(from);
        if self.allows_forward(to.seg as usize) {
            best = best.min(dist[to_seg.node_a] + to.t * to_seg.len);
        }
        if self.allows_backward(to.seg as usize) {
            best = best.min(dist[to_seg.node_b] + (1.0 - to.t) * to_seg.len);
        }
        if best.is_finite() {
            Some(best)
        } else {
            None
        }
    }
}

/// The routed path's total cost: the walk-in from the position to the
/// first path point plus every hop.
fn path_total_cost(from: StreetPosition, path: &[transit_streets::StreetPoint]) -> f64 {
    let mut total = 0.0;
    let mut prev = (from.lat, from.lon);
    for p in path {
        total += meters_between(prev.0, prev.1, p.lat, p.lon);
        prev = (p.lat, p.lon);
    }
    total
}

/// A query anchored near a street: interpolated on segment `seg` at
/// `t`, plus a jitter well under one cell.
fn near_query(n_segs: usize) -> impl Strategy<Value = (usize, f64, f64)> {
    (0usize..n_segs, 0.0f64..1.0, -0.3f64..0.3)
}

fn query_point(g: &OGraph, q: (usize, f64, f64)) -> (f64, f64) {
    let seg = &g.segs[q.0];
    let lat = seg.a.0 + q.1 * (seg.b.0 - seg.a.0) + q.2 * CELL;
    let lon = seg.a.1 + q.1 * (seg.b.1 - seg.a.1) + q.2 * CELL;
    (lat, lon)
}

fn spec_and_queries() -> impl Strategy<Value = (Spec, (usize, f64, f64), (usize, f64, f64), (i64, i64))> {
    spec()
        .prop_filter("spec has at least one non-degenerate segment", |spec| {
            !build_oracle(spec).segs.is_empty()
        })
        .prop_flat_map(|spec| {
            let g = build_oracle(&spec);
            let n = g.segs.len();
            (Just(spec), near_query(n), near_query(n), (20i64..40, 20i64..40))
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Structure: the parsed graph agrees with the spec on segment and
    /// node counts — welding and degenerate-row dropping included. A
    /// spec with no usable geometry is an error, not a graph.
    #[test]
    fn graph_shape_matches_the_spec(spec in spec()) {
        let g = build_oracle(&spec);
        let parsed = StreetNetwork::parse(build_geojson(&spec).as_bytes());
        if g.segs.is_empty() {
            prop_assert!(parsed.is_err(), "no usable line geometry must error");
        } else {
            let net = parsed.expect("generated GeoJSON parses");
            prop_assert_eq!(net.segment_count() as usize, g.segs.len());
            prop_assert_eq!(net.node_count() as usize, g.nodes.len());
        }
    }

    /// Snap: the position lies on its segment, dist_m is true, and
    /// re-snapping the snapped point is a fixed point.
    #[test]
    fn snap_projects_and_is_a_fixed_point(
        (spec, qa, qb, far) in spec_and_queries(),
    ) {
        let g = build_oracle(&spec);
        let net = StreetNetwork::parse(build_geojson(&spec).as_bytes())
            .expect("parse");
        let (lat, lon) = query_point(&g, qa);
        let pos = net.snap(lat, lon).expect("a query anchored on a street snaps");

        prop_assert!(g.distance_to_segment(pos.lat, pos.lon, pos.seg as usize) <= 1e-6,
            "snapped point off its segment");
        // dist_m is the true projection distance in the module's own
        // metric (queried-latitude cos), not the midpoint-cos metric.
        let expected = g.snap_distance(lat, lon).expect("oracle finds the street too");
        prop_assert!(close(pos.dist_m, expected.2, 1e-6));

        let again = net.snap(pos.lat, pos.lon).expect("a street point snaps");
        prop_assert!(close(again.lat, pos.lat, 1e-9));
        prop_assert!(close(again.lon, pos.lon, 1e-9));
        prop_assert!(again.dist_m <= 1e-6);

        // A point well outside every window cell: no street nearby.
        let far_lat = LAT0 + far.0 as f64 * CELL;
        let far_lon = LON0 + far.1 as f64 * CELL;
        prop_assert!(net.snap(far_lat, far_lon).is_none());
        prop_assert!(g.snap_distance(far_lat, far_lon).is_none());
        let _ = qb;
    }

    /// Snap nearest: dist_m matches the brute-force nearest
    /// projection over the whole visible window.
    #[test]
    fn snap_distance_matches_the_brute_force_oracle(
        (spec, qa, qb, far) in spec_and_queries(),
    ) {
        let g = build_oracle(&spec);
        let net = StreetNetwork::parse(build_geojson(&spec).as_bytes())
            .expect("parse");
        for q in [qa, qb] {
            let (lat, lon) = query_point(&g, q);
            let got = net.snap(lat, lon).expect("anchored query snaps");
            let expected = g.snap_distance(lat, lon).expect("oracle finds the street too");
            prop_assert!(close(got.dist_m, expected.2, 1e-6));
        }
        let _ = far;
    }

    /// Route: the path's total cost equals the oracle's optimal cost,
    /// None means unreachable, the path is never empty, it ends
    /// exactly at the target, and it is deterministic.
    #[test]
    fn route_cost_matches_the_dijkstra_oracle(
        (spec, qa, qb, _far) in spec_and_queries(),
    ) {
        let g = build_oracle(&spec);
        let net = StreetNetwork::parse(build_geojson(&spec).as_bytes())
            .expect("parse");
        let (flat, flon) = query_point(&g, qa);
        let (tlat, tlon) = query_point(&g, qb);
        let from = net.snap(flat, flon).expect("from snaps");
        let to = net.snap(tlat, tlon).expect("to snaps");

        let expected = g.shortest_cost(from, to);
        let got = net.route(from, to);

        match (got, expected) {
            (None, None) => {}
            (Some(path), Some(cost)) => {
                prop_assert!(!path.is_empty());
                prop_assert_eq!(path.last().map(|p| (p.lat, p.lon)), Some((to.lat, to.lon)));
                let total = path_total_cost(from, &path);
                // The path cost is measured hop-by-hop with the
                // midpoint-cos metric; the oracle sums per-segment
                // lengths. The two agree to ~1e-6 relative — anything
                // beyond that is a genuinely suboptimal path.
                prop_assert!(
                    close(total, cost, 1e-4 * (1.0 + cost)),
                    "routed {total} m, oracle says {cost} m"
                );
                // The reported length IS the search's cost: the
                // oracle's shortest distance, not a re-sum.
                let (_, length) = net.route_with_length(from, to).expect("same pair routes");
                prop_assert!(close(length, cost, 1e-6 * (1.0 + cost)), "length {length} m, oracle {cost} m");
                // Determinism: same instance, same answer.
                prop_assert_eq!(net.route(from, to).map(|p| (p.len(), path_total_cost(from, &p))),
                               Some((path.len(), total)));
            }
            (None, Some(cost)) => panic!("route gave up; oracle reaches for {cost} m"),
            (Some(_), None) => panic!("route returned a path on an unreachable pair"),
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Totality: arbitrary bytes never panic the parser — Ok or Err.
    #[test]
    fn parse_survives_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..=128)) {
        let _ = StreetNetwork::parse(&bytes);
    }
}
