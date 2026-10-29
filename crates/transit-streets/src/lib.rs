//! The streets model: the street network as a document, its import,
//! and the routing graph over it.
//!
//! The document ([`Streets`], `model.rs`) is the model; a street
//! layer (an agency GIS export, TIGER, OSM — GeoJSON lines) is a
//! serialization read into it by [`geojson::import`] at the boundary.
//! The graph ([`StreetNetwork`]) is built from the document, and its
//! queries answer the needs that run on streets: a detour traced
//! between waypoints, a pattern drawn the same way, the distance by
//! road. A trace ROUTES along the streets, so committed geometry is
//! street geometry, never freehand chords.
//!
//! Pure and stateless: no I/O, no clock, no shared state — the
//! cap-streets capability registers documents under their content
//! hash, hosts keep the documents.

pub mod geojson;
mod model;
pub mod walking;

pub use model::{StreetClass, Streets, Way};

use std::collections::{BinaryHeap, HashMap};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Grid cell size in degrees (~110 m): snapping thresholds are
/// smaller, so a 3x3 neighborhood of the click's cell always holds
/// every candidate segment.
const CELL: f64 = 0.001;

/// A position on a street: the projected point plus its graph
/// address (segment id, parameter along it from the segment's first
/// vertex). Segments are numbered through the document, way by way
/// and point pair by point pair, so an address holds for one streets
/// document (one hash) — it is never committed to another document.
/// `dist_m` is how far the query point was from the street.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StreetPosition {
    pub lat: f64,
    pub lon: f64,
    pub dist_m: f64,
    pub seg: u32,
    pub t: f64,
}

/// A [lat, lon] point of a routed path.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct StreetPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug)]
pub struct StreetError(pub String);

/// Equirectangular meters between two positions — accurate at the
/// street scales this module works at.
fn meters_between(a_lat: f64, a_lon: f64, b_lat: f64, b_lon: f64) -> f64 {
    let cos_lat = ((a_lat + b_lat) / 2.0).to_radians().cos();
    let dy = (b_lat - a_lat) * 110_540.0;
    let dx = (b_lon - a_lon) * 111_320.0 * cos_lat;
    dx.hypot(dy)
}

fn cell_key(ix: i64, iy: i64) -> i64 {
    (ix + 200_000) * 400_000 + (iy + 100_000)
}

/// Incremental graph construction from the document's ways.
struct Builder {
    coords: Vec<f64>,
    seg_len: Vec<f64>,
    seg_node: Vec<u32>,
    seg_oneway: Vec<bool>,
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

    fn add_segment(&mut self, a: [i32; 2], b: [i32; 2], oneway: bool) {
        let (a_lat, a_lon) = (a[0] as f64 / 1e6, a[1] as f64 / 1e6);
        let (b_lat, b_lon) = (b[0] as f64 / 1e6, b[1] as f64 / 1e6);
        let id = self.coords.len() as u32 / 4;
        self.coords.extend_from_slice(&[a_lat, a_lon, b_lat, b_lon]);
        self.seg_len.push(meters_between(a_lat, a_lon, b_lat, b_lon));
        self.seg_oneway.push(oneway);
        let node_a = self.node_of(a);
        let node_b = self.node_of(b);
        self.seg_node.extend_from_slice(&[node_a, node_b]);
        // Register in every cell the segment's bbox touches, so long
        // segments are found anywhere along them.
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

/// The routing graph over a streets document: nodes are the
/// document's points (microdegrees — ways that share a point weld
/// there), edges are segments, adjacency in CSR form. Cells index
/// segments by bbox for snapping.
pub struct StreetNetwork {
    /// Flat segment endpoints, 4 floats per segment
    /// `[aLat, aLon, bLat, bLon, ..]`.
    coords: Vec<f64>,
    seg_len: Vec<f64>,
    seg_node: Vec<u32>,
    /// Per segment: travel only from its first point to its second.
    seg_oneway: Vec<bool>,
    node_coords: Vec<f64>,
    adj_offset: Vec<u32>,
    adj_seg: Vec<u32>,
    adj_node: Vec<u32>,
    cells: HashMap<i64, Vec<u32>>,
    segment_count: u32,
    node_count: u32,
}

impl StreetNetwork {
    /// The graph over a streets document. Segments are numbered way
    /// by way, point pair by point pair — the address a
    /// [`StreetPosition`] names.
    pub fn build(streets: &Streets) -> StreetNetwork {
        let mut b = Builder {
            coords: Vec::new(),
            seg_len: Vec::new(),
            seg_node: Vec::new(),
            seg_oneway: Vec::new(),
            node_coords: Vec::new(),
            nodes: HashMap::new(),
            cells: HashMap::new(),
        };
        for way in &streets.ways {
            for pair in way.points.windows(2) {
                b.add_segment(pair[0], pair[1], way.oneway);
            }
        }
        let segment_count = b.coords.len() as u32 / 4;

        // CSR adjacency: each segment contributes the edges its
        // direction allows — a one-way enters the graph as a
        // first-to-second edge only, so Dijkstra can only follow legal
        // travel.
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

    /// The nearest point on any street segment to (lat, lon), within
    /// the 3x3 cell neighborhood. None when no street is nearby.
    pub fn snap(&self, lat: f64, lon: f64) -> Option<StreetPosition> {
        let ix = (lon / CELL).floor() as i64;
        let iy = (lat / CELL).floor() as i64;
        let m_lat = 110_540.0;
        let m_lon = 111_320.0 * lat.to_radians().cos();

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
                    // Project the query point onto the segment in
                    // local meters.
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

    /// The street geometry between two positions: the shortest path
    /// along the graph, as points ending exactly at `to`. The path
    /// starts after `from` (its slice's end is already committed).
    /// None when the positions sit on disconnected parts of the
    /// network — the caller falls back to a straight line.
    pub fn route(&self, from: StreetPosition, to: StreetPosition) -> Option<Vec<StreetPoint>> {
        self.route_with_length(from, to).map(|(points, _)| points)
    }

    /// [`route`](Self::route) plus the path's length along the graph
    /// in meters — from the `from` position itself (its walk to the
    /// segment's exit counts) to the exact `to` position: the
    /// shortest-path cost the search settled on, so "how far by
    /// road?" is the router's own answer, never a host-side sum.
    pub fn route_with_length(&self, from: StreetPosition, to: StreetPosition) -> Option<(Vec<StreetPoint>, f64)> {
        let from_oneway = self.seg_oneway[from.seg as usize];
        let to_oneway = self.seg_oneway[to.seg as usize];

        // Same segment: the segment is straight — but only when the
        // direction allows reaching a target that sits behind the
        // position. On a one-way the route must leave and come back,
        // which is the Dijkstra below.
        if from.seg == to.seg {
            let reachable = !from_oneway || to.t >= from.t;
            if reachable {
                let length = (to.t - from.t).abs() * self.seg_len[from.seg as usize];
                return Some((vec![StreetPoint { lat: to.lat, lon: to.lon }], length));
            }
        }

        // The two ends of the target segment act as virtual edges to
        // the exact position: entering the segment at nodeA leaves
        // t*len to walk, at nodeB (1-t)*len. Entering is itself
        // travel — nodeA always, nodeB only on a two-way.
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

        // Multi-source Dijkstra over a binary heap. The sources are
        // both ends of the from-segment with their walk-in costs;
        // their parent pointers stay at the sentinel (-1): the walk
        // from the position to its segment's end is inside a
        // straight segment and needs no committed geometry of its
        // own. Heap costs are finite non-negative, so the bit
        // pattern order equals the value order.
        let n = self.node_count as usize;
        let mut dist = vec![f64::INFINITY; n];
        let mut parent_seg = vec![-1i32; n];
        let mut parent_node = vec![-1i32; n];
        let mut heap: BinaryHeap<std::cmp::Reverse<(u64, usize)>> = BinaryHeap::new();
        let from_len = self.seg_len[from.seg as usize];
        let from_a = self.seg_node[2 * from.seg as usize] as usize;
        let from_b = self.seg_node[2 * from.seg as usize + 1] as usize;
        // Leaving toward the second point is travel with the
        // segment; toward the first point is travel against it.
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
            // A target-segment end is a candidate entry; keep
            // expanding — the other end may still be cheaper.
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

        // The committed geometry is the chain of reached nodes, from
        // the from-segment's exit node to the target's entry node —
        // every bend on the path — then the exact target position.
        // The terminal seed IS the from-segment's exit bend: it must
        // be committed too (callers dedupe against the previous
        // trace point when the position sits on that node).
        let mut chain: Vec<usize> = Vec::new();
        let mut node = best_entry as usize;
        loop {
            chain.push(node);
            if parent_seg[node] < 0 {
                break; // seed: entered via the from-segment
            }
            node = parent_node[node] as usize;
        }
        chain.reverse();
        let mut points: Vec<StreetPoint> = chain
            .into_iter()
            .map(|id| StreetPoint {
                lat: self.node_coords[2 * id],
                lon: self.node_coords[2 * id + 1],
            })
            .collect();
        points.push(StreetPoint { lat: to.lat, lon: to.lon });
        Some((points, best_cost))
    }

    /// Street meters from `from` to every node within `limit_m`, into
    /// `dist` (one slot per node, infinite where unreached). The nodes
    /// it sets are appended to `touched`, so a caller searching from
    /// many positions resets only those. Travel honors one-way.
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

    /// Street meters from `from` to the position `to`, over the node
    /// distances [`reach`](Self::reach) left from `from` — infinite
    /// when `to` is out of reach.
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
