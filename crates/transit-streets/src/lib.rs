//! [`geojson::import`] is the only code that reads a street layer's
//! properties. Which source a deployment loads is its choice: an agency's
//! roads give scheduling parity, and OSM gives sidewalks. Walking
//! quality is the source's, and C-TRAN's roads cannot carry it (QAF-85).

pub mod bus;
pub mod geojson;
mod model;
pub mod walking;
pub mod load;

pub use model::{StreetClass, Streets, Way};

use std::collections::{BinaryHeap, HashMap};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Grid cell size in degrees (about 110 m). Snapping thresholds are smaller,
/// so the 3x3 cells around a point hold every candidate segment.
const CELL: f64 = 0.001;

/// `seg` and `t` (the parameter from the segment's first vertex) address a
/// segment of one streets document, numbered way by way and point pair by
/// point pair. Never commit them to another document.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StreetPosition {
    pub lat: f64,
    pub lon: f64,
    pub dist_m: f64,
    pub seg: u32,
    pub t: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StreetPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug)]
pub struct StreetError(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StreetUse {
    pub class: StreetClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    pub length_m: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreetRoute {
    pub points: Vec<StreetPoint>,
    pub length_m: f64,
    pub streets: Vec<StreetUse>,
}

fn cell_key(ix: i64, iy: i64) -> i64 {
    (ix + 200_000) * 400_000 + (iy + 100_000)
}

struct Builder {
    coords: Vec<f64>,
    seg_len: Vec<f64>,
    seg_node: Vec<u32>,
    seg_oneway: Vec<bool>,
    seg_way: Vec<u32>,
    node_coords: Vec<f64>,
    nodes: HashMap<[i32; 2], u32>,
    cells: HashMap<i64, Vec<u32>>,
}

impl Builder {
    fn node_of(&mut self, p: [i32; 2]) -> u32 {
        let next = self.node_coords.len() as u32 / 2;
        *self.nodes.entry(p).or_insert_with(|| {
            self.node_coords.push(p[0] as f64 / 1e6);
            self.node_coords.push(p[1] as f64 / 1e6);
            next
        })
    }

    fn add_segment(&mut self, a: [i32; 2], b: [i32; 2], oneway: bool, way: u32) {
        let (a_lat, a_lon) = (a[0] as f64 / 1e6, a[1] as f64 / 1e6);
        let (b_lat, b_lon) = (b[0] as f64 / 1e6, b[1] as f64 / 1e6);
        let id = self.coords.len() as u32 / 4;
        self.coords.extend_from_slice(&[a_lat, a_lon, b_lat, b_lon]);
        self.seg_len.push(transit_geo::distance_m(a_lat, a_lon, b_lat, b_lon));
        self.seg_oneway.push(oneway);
        self.seg_way.push(way);
        let node_a = self.node_of(a);
        let node_b = self.node_of(b);
        self.seg_node.extend_from_slice(&[node_a, node_b]);
        // Register in every cell that the bbox touches, so a long segment
        // is found anywhere along it.
        let ix0 = (a_lon.min(b_lon) / CELL).floor() as i64;
        let ix1 = (a_lon.max(b_lon) / CELL).floor() as i64;
        let iy0 = (a_lat.min(b_lat) / CELL).floor() as i64;
        let iy1 = (a_lat.max(b_lat) / CELL).floor() as i64;
        for ix in ix0..=ix1 {
            for iy in iy0..=iy1 {
                self.cells.entry(cell_key(ix, iy)).or_default().push(id);
            }
        }
    }
}

pub struct StreetNetwork {
    /// Each segment has four floats, in the order a lat, a lon, b lat, b lon.
    coords: Vec<f64>,
    seg_len: Vec<f64>,
    seg_node: Vec<u32>,
    seg_oneway: Vec<bool>,
    seg_way: Vec<u32>,
    way_street: Vec<(StreetClass, Option<String>)>,
    node_coords: Vec<f64>,
    adj_offset: Vec<u32>,
    adj_seg: Vec<u32>,
    adj_node: Vec<u32>,
    cells: HashMap<i64, Vec<u32>>,
    segment_count: u32,
    node_count: u32,
}

impl StreetNetwork {
    pub fn build(streets: &Streets) -> StreetNetwork {
        let mut b = Builder {
            coords: Vec::new(),
            seg_len: Vec::new(),
            seg_node: Vec::new(),
            seg_oneway: Vec::new(),
            seg_way: Vec::new(),
            node_coords: Vec::new(),
            nodes: HashMap::new(),
            cells: HashMap::new(),
        };
        for (w, way) in streets.ways.iter().enumerate() {
            for pair in way.points.windows(2) {
                b.add_segment(pair[0], pair[1], way.oneway, w as u32);
            }
        }
        let way_street = streets.ways.iter().map(|w| (w.class, w.name.clone())).collect();
        let segment_count = b.coords.len() as u32 / 4;

        // A one-way enters the graph as a first-to-second edge only, so
        // Dijkstra follows only legal travel.
        let node_count = b.node_coords.len() as u32 / 2;
        let mut degree = vec![0u32; node_count as usize];
        for i in 0..segment_count as usize {
            degree[b.seg_node[2 * i] as usize] += 1;
            if !b.seg_oneway[i] {
                degree[b.seg_node[2 * i + 1] as usize] += 1;
            }
        }
        let mut adj_offset = Vec::with_capacity(node_count as usize + 1);
        let mut acc = 0u32;
        for d in &degree {
            adj_offset.push(acc);
            acc += *d;
        }
        adj_offset.push(acc);
        let mut cursor = adj_offset.clone();
        let mut adj_seg = vec![0u32; acc as usize];
        let mut adj_node = vec![0u32; acc as usize];
        for i in 0..segment_count as usize {
            let a = b.seg_node[2 * i] as usize;
            let bn = b.seg_node[2 * i + 1] as usize;
            let ca = cursor[a] as usize;
            adj_seg[ca] = i as u32;
            adj_node[ca] = b.seg_node[2 * i + 1];
            cursor[a] += 1;
            if !b.seg_oneway[i] {
                let cb = cursor[bn] as usize;
                adj_seg[cb] = i as u32;
                adj_node[cb] = b.seg_node[2 * i];
                cursor[bn] += 1;
            }
        }

        StreetNetwork {
            coords: b.coords,
            seg_len: b.seg_len,
            seg_node: b.seg_node,
            seg_oneway: b.seg_oneway,
            seg_way: b.seg_way,
            way_street,
            node_coords: b.node_coords,
            adj_offset,
            adj_seg,
            adj_node,
            cells: b.cells,
            segment_count,
            node_count,
        }
    }

    pub fn segment_count(&self) -> u32 {
        self.segment_count
    }

    pub fn node_count(&self) -> u32 {
        self.node_count
    }

    pub fn snap(&self, lat: f64, lon: f64) -> Option<StreetPosition> {
        let ix = (lon / CELL).floor() as i64;
        let iy = (lat / CELL).floor() as i64;
        let m_lat = transit_geo::METERS_PER_DEGREE;
        let m_lon = transit_geo::meters_per_degree_lon(lat);

        let mut best: Option<StreetPosition> = None;
        for dx in -1i64..=1 {
            for dy in -1i64..=1 {
                let Some(bucket) = self.cells.get(&cell_key(ix + dx, iy + dy)) else {
                    continue;
                };
                for &id in bucket {
                    let o = id as usize * 4;
                    let a_lat = self.coords[o];
                    let a_lon = self.coords[o + 1];
                    let b_lat = self.coords[o + 2];
                    let b_lon = self.coords[o + 3];
                    let ax = (a_lon - lon) * m_lon;
                    let ay = (a_lat - lat) * m_lat;
                    let bx = (b_lon - lon) * m_lon;
                    let by = (b_lat - lat) * m_lat;
                    let len2 = (bx - ax) * (bx - ax) + (by - ay) * (by - ay);
                    let t = if len2 == 0.0 {
                        0.0
                    } else {
                        (-(ax * (bx - ax) + ay * (by - ay)) / len2).clamp(0.0, 1.0)
                    };
                    let px = ax + t * (bx - ax);
                    let py = ay + t * (by - ay);
                    let dist_m = px.hypot(py);
                    if best.is_none_or(|b| dist_m < b.dist_m) {
                        best = Some(StreetPosition {
                            lat: lat + py / m_lat,
                            lon: lon + px / m_lon,
                            dist_m,
                            seg: id,
                            t,
                        });
                    }
                }
            }
        }
        best
    }

    pub fn route(&self, from: StreetPosition, to: StreetPosition) -> Option<Vec<StreetPoint>> {
        self.route_with_length(from, to).map(|(points, _)| points)
    }

    pub fn route_with_length(&self, from: StreetPosition, to: StreetPosition) -> Option<(Vec<StreetPoint>, f64)> {
        self.route_along(from, to).map(|r| (r.points, r.length_m))
    }

    /// The path never depends on street class. A profile ([`bus`]) reads the
    /// classes of the streets that it follows.
    pub fn route_along(&self, from: StreetPosition, to: StreetPosition) -> Option<StreetRoute> {
        let (points, length_m, segments) = self.shortest(from, to)?;
        let mut streets: Vec<StreetUse> = Vec::new();
        for (seg, meters) in segments {
            if meters <= 0.0 {
                continue;
            }
            let (class, name) = &self.way_street[self.seg_way[seg as usize] as usize];
            match streets.last_mut() {
                Some(last) if last.class == *class && last.name == *name => last.length_m += meters,
                _ => streets.push(StreetUse { class: *class, name: name.clone(), length_m: meters }),
            }
        }
        Some(StreetRoute { points, length_m, streets })
    }

    /// The segment meters, partial first and last included, add up to the
    /// length.
    fn shortest(&self, from: StreetPosition, to: StreetPosition) -> Option<(Vec<StreetPoint>, f64, Vec<(u32, f64)>)> {
        let from_oneway = self.seg_oneway[from.seg as usize];
        let to_oneway = self.seg_oneway[to.seg as usize];

        // On a one-way, a target behind the position must leave and come
        // back, which the Dijkstra below does.
        if from.seg == to.seg {
            let reachable = !from_oneway || to.t >= from.t;
            if reachable {
                let length = (to.t - from.t).abs() * self.seg_len[from.seg as usize];
                return Some((vec![StreetPoint { lat: to.lat, lon: to.lon }], length, vec![(from.seg, length)]));
            }
        }

        // Entering the target segment is itself travel: at nodeA always,
        // at nodeB only on a two-way.
        let to_len = self.seg_len[to.seg as usize];
        let to_a = self.seg_node[2 * to.seg as usize];
        let to_b = self.seg_node[2 * to.seg as usize + 1];
        let inside_cost = |node: u32| -> Option<f64> {
            if node == to_a {
                Some(to.t * to_len)
            } else if node == to_b && !to_oneway {
                Some((1.0 - to.t) * to_len)
            } else {
                None
            }
        };

        // The sources are both ends of the from-segment. Their parent
        // pointers stay at the sentinel (-1), because the walk to a
        // segment end needs no committed geometry. Heap costs are finite
        // and non-negative, so bit-pattern order equals value order.
        let n = self.node_count as usize;
        let mut dist = vec![f64::INFINITY; n];
        let mut parent_seg = vec![-1i32; n];
        let mut parent_node = vec![-1i32; n];
        let mut heap: BinaryHeap<std::cmp::Reverse<(u64, usize)>> = BinaryHeap::new();
        let from_len = self.seg_len[from.seg as usize];
        let from_a = self.seg_node[2 * from.seg as usize] as usize;
        let from_b = self.seg_node[2 * from.seg as usize + 1] as usize;
        let mut starts: Vec<(f64, usize)> = vec![((1.0 - from.t) * from_len, from_b)];
        if !from_oneway {
            starts.push((from.t * from_len, from_a));
        }
        for (cost, node) in starts {
            if cost < dist[node] {
                dist[node] = cost;
                heap.push(std::cmp::Reverse((cost.to_bits(), node)));
            }
        }

        let mut best_entry: i64 = -1;
        let mut best_cost = f64::INFINITY;
        while let Some(std::cmp::Reverse((bits, node))) = heap.pop() {
            let d = f64::from_bits(bits);
            if d > best_cost {
                break; // everything left is worse
            }
            // Keep expanding after a candidate entry, because the other end
            // of the target segment may be cheaper.
            if let Some(inside) = inside_cost(node as u32) {
                if d + inside < best_cost {
                    best_cost = d + inside;
                    best_entry = node as i64;
                }
            }
            for e in self.adj_offset[node]..self.adj_offset[node + 1] {
                let seg = self.adj_seg[e as usize] as usize;
                let next = self.adj_node[e as usize] as usize;
                let nd = d + self.seg_len[seg];
                if nd < dist[next] {
                    dist[next] = nd;
                    parent_seg[next] = seg as i32;
                    parent_node[next] = node as i32;
                    heap.push(std::cmp::Reverse((nd.to_bits(), next)));
                }
            }
        }
        if best_entry < 0 {
            return None;
        }

        // The from-segment's exit node is committed too. Callers dedupe it
        // against the previous trace point when the position sits on it.
        let mut chain: Vec<usize> = Vec::new();
        let mut segments: Vec<(u32, f64)> = Vec::new();
        let mut node = best_entry as usize;
        loop {
            chain.push(node);
            if parent_seg[node] < 0 {
                break; // seed: entered via the from-segment
            }
            let seg = parent_seg[node] as usize;
            segments.push((seg as u32, self.seg_len[seg]));
            node = parent_node[node] as usize;
        }
        chain.reverse();
        let seed_cost = if node == from_b { (1.0 - from.t) * from_len } else { from.t * from_len };
        segments.push((from.seg, seed_cost));
        segments.reverse();
        segments.push((to.seg, inside_cost(best_entry as u32).unwrap_or(0.0)));
        let mut points: Vec<StreetPoint> = chain
            .into_iter()
            .map(|id| StreetPoint {
                lat: self.node_coords[2 * id],
                lon: self.node_coords[2 * id + 1],
            })
            .collect();
        points.push(StreetPoint { lat: to.lat, lon: to.lon });
        Some((points, best_cost, segments))
    }

    /// `dist` has one slot per node, infinite where unreached. `touched`
    /// collects the nodes that it sets, so a caller that searches from many
    /// positions resets only those.
    pub(crate) fn reach(&self, from: StreetPosition, limit_m: f64, dist: &mut [f64], touched: &mut Vec<u32>) {
        let seg = from.seg as usize;
        let len = self.seg_len[seg];
        let mut heap: BinaryHeap<std::cmp::Reverse<(u64, u32)>> = BinaryHeap::new();
        let mut seed = |node: u32, cost: f64, dist: &mut [f64], heap: &mut BinaryHeap<_>| {
            if cost <= limit_m && cost < dist[node as usize] {
                if dist[node as usize].is_infinite() {
                    touched.push(node);
                }
                dist[node as usize] = cost;
                heap.push(std::cmp::Reverse((cost.to_bits(), node)));
            }
        };
        seed(self.seg_node[2 * seg + 1], (1.0 - from.t) * len, dist, &mut heap);
        if !self.seg_oneway[seg] {
            seed(self.seg_node[2 * seg], from.t * len, dist, &mut heap);
        }
        while let Some(std::cmp::Reverse((bits, node))) = heap.pop() {
            let d = f64::from_bits(bits);
            if d > dist[node as usize] {
                continue;
            }
            for e in self.adj_offset[node as usize]..self.adj_offset[node as usize + 1] {
                let next = self.adj_node[e as usize];
                seed(next, d + self.seg_len[self.adj_seg[e as usize] as usize], dist, &mut heap);
            }
        }
    }

    /// Infinite when `to` is out of reach of the `dist` that `reach` left.
    pub(crate) fn meters_to(&self, from: StreetPosition, dist: &[f64], to: StreetPosition) -> f64 {
        let seg = to.seg as usize;
        let len = self.seg_len[seg];
        let mut best = dist[self.seg_node[2 * seg] as usize] + to.t * len;
        if !self.seg_oneway[seg] {
            best = best.min(dist[self.seg_node[2 * seg + 1] as usize] + (1.0 - to.t) * len);
        }
        if from.seg == to.seg && (!self.seg_oneway[seg] || to.t >= from.t) {
            best = best.min((to.t - from.t).abs() * len);
        }
        best
    }
}
