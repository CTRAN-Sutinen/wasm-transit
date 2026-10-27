//! Street networks for detour authoring.
//!
//! A dispatcher traces a detour with waypoints; the trace between
//! waypoints ROUTES along an imported street network (an OSM extract
//! or an open-data layer as GeoJSON lines), so the committed detour
//! polyline is street geometry, never freehand chords.
//!
//! The module is pure and stateless: `parse` turns bytes into a
//! [`StreetNetwork`] graph, `snap`/`route` are queries over it. No
//! I/O, no clock, no shared state — the cap-streets capability keeps
//! a content-hash registry over parsed graphs, hosts keep the bytes.
//! This is deliberately the "stateless capability" shape discussed
//! for future module splits: `load(bytes) -> hash` + queries, never
//! document state.

use std::collections::{BinaryHeap, HashMap};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Grid cell size in degrees (~110 m): snapping thresholds are
/// smaller, so a 3x3 neighborhood of the click's cell always holds
/// every candidate segment.
const CELL: f64 = 0.001;

/// A position on a street: the projected point plus its graph
/// address (segment id, parameter along it from the segment's first
/// vertex). `dist_m` is how far the query point was from the street.
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

/// Traversal direction of a street relative to its digitized
/// orientation (first vertex to last).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Both,
    /// Travel only first-to-last (TIGER `OneWay: ">"`, OSM `oneway: yes`).
    Forward,
    /// Travel only last-to-first (TIGER `OneWay: "<"`, OSM `oneway: -1`).
    Backward,
}

impl Dir {
    const BOTH: u8 = 0;
    const FORWARD: u8 = 1;
    const BACKWARD: u8 = 2;

    fn code(self) -> u8 {
        match self {
            Dir::Both => Self::BOTH,
            Dir::Forward => Self::FORWARD,
            Dir::Backward => Self::BACKWARD,
        }
    }

    fn from_code(code: u8) -> Dir {
        match code {
            Self::FORWARD => Dir::Forward,
            Self::BACKWARD => Dir::Backward,
            _ => Dir::Both,
        }
    }

    fn allows_forward(self) -> bool {
        !matches!(self, Dir::Backward)
    }

    fn allows_backward(self) -> bool {
        !matches!(self, Dir::Forward)
    }
}

/// One-way convention from a feature's properties, read case-
/// insensitively: TIGER-style `OneWay` (`>` with the digitized
/// direction, `<` against; some derivatives use F/T, FT/TF) and
/// OSM-style `oneway` (`yes`/`true`/`1` with, `-1`/`reverse`
/// against, `no`/`false`/`0` two-way). Unrecognized values are
/// two-way: schemas drift, and a wrong one-way is worse than none.
fn feature_dir(props: Option<&serde_json::Value>) -> Dir {
    let Some(props) = props else {
        return Dir::Both;
    };
    let raw = ["OneWay", "oneway", "ONEWAY"]
        .iter()
        .find_map(|k| props.get(k))
        .and_then(|v| v.as_str());
    let Some(raw) = raw else {
        return Dir::Both;
    };
    match raw.trim().to_ascii_uppercase().as_str() {
        ">" | "F" | "FT" | "YES" | "TRUE" | "1" => Dir::Forward,
        "<" | "T" | "TF" | "REVERSE" | "-1" => Dir::Backward,
        _ => Dir::Both,
    }
}

/// Incremental graph construction while walking the GeoJSON rows.
struct Builder {
    coords: Vec<f64>,
    seg_len: Vec<f64>,
    seg_node: Vec<u32>,
    seg_dir: Vec<u8>,
    node_coords: Vec<f64>,
    nodes: HashMap<(i64, i64), u32>,
    cells: HashMap<i64, Vec<u32>>,
}

impl Builder {
    fn node_of(&mut self, lat: f64, lon: f64) -> u32 {
        let key = ((lat * 1e6).round() as i64, (lon * 1e6).round() as i64);
        let next = self.node_coords.len() as u32 / 2;
        *self.nodes.entry(key).or_insert_with(|| {
            self.node_coords.push(lat);
            self.node_coords.push(lon);
            next
        })
    }

    fn add_segment(&mut self, a_lat: f64, a_lon: f64, b_lat: f64, b_lon: f64, dir: Dir) {
        let len = meters_between(a_lat, a_lon, b_lat, b_lon);
        if len <= 0.0 {
            return; // degenerate rows weld noise, not streets
        }
        let id = self.coords.len() as u32 / 4;
        self.coords.extend_from_slice(&[a_lat, a_lon, b_lat, b_lon]);
        self.seg_len.push(len);
        self.seg_dir.push(dir.code());
        let a = self.node_of(a_lat, a_lon);
        let b = self.node_of(b_lat, b_lon);
        self.seg_node.extend_from_slice(&[a, b]);
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

    /// One line's vertices, consecutive pairs as segments. Bad rows
    /// break the chain (a gap, not a segment) like the source data.
    fn add_line(&mut self, line: &serde_json::Value, dir: Dir) {
        let mut prev: Option<(f64, f64)> = None;
        for pt in line.as_array().into_iter().flatten() {
            let row = pt.as_array();
            let point = row
                .and_then(|a| a.first().and_then(|x| x.as_f64()))
                .zip(row.and_then(|a| a.get(1).and_then(|y| y.as_f64())));
            // GeoJSON order: [lon, lat].
            let Some((lon, lat)) = point else {
                prev = None;
                continue;
            };
            if !lat.is_finite() || !lon.is_finite() {
                prev = None;
                continue;
            }
            if let Some((p_lat, p_lon)) = prev {
                self.add_segment(p_lat, p_lon, lat, lon, dir);
            }
            prev = Some((lat, lon));
        }
    }
}

/// The routing graph: nodes are shared (quantized ~0.1 m) segment
/// endpoints — intersections weld — edges are segments, adjacency in
/// CSR form. Cells index segments by bbox for snapping.
pub struct StreetNetwork {
    /// Flat segment endpoints, 4 floats per segment
    /// `[aLat, aLon, bLat, bLon, ..]`.
    coords: Vec<f64>,
    seg_len: Vec<f64>,
    seg_node: Vec<u32>,
    /// Per-segment [`Dir`] code.
    seg_dir: Vec<u8>,
    node_coords: Vec<f64>,
    adj_offset: Vec<u32>,
    adj_seg: Vec<u32>,
    adj_node: Vec<u32>,
    cells: HashMap<i64, Vec<u32>>,
    segment_count: u32,
    node_count: u32,
}

impl StreetNetwork {
    /// Parse a GeoJSON FeatureCollection (or bare Feature) of
    /// LineString / MultiLineString geometries. Points, polygons and
    /// malformed rows are skipped; degenerate (zero-length) segments
    /// are dropped. Errors when the input is not GeoJSON or carries
    /// no usable line geometry.
    pub fn parse(geojson: &[u8]) -> Result<StreetNetwork, StreetError> {
        let value: serde_json::Value = serde_json::from_slice(geojson)
            .map_err(|e| StreetError(format!("invalid JSON: {e}")))?;
        let features = match value.get("type").and_then(|t| t.as_str()) {
            Some("FeatureCollection") => value
                .get("features")
                .and_then(|f| f.as_array())
                .cloned()
                .unwrap_or_default(),
            Some("Feature") => vec![value],
            _ => {
                return Err(StreetError(
                    "expected a GeoJSON FeatureCollection or Feature".into(),
                ))
            }
        };

        let mut b = Builder {
            coords: Vec::new(),
            seg_len: Vec::new(),
            seg_node: Vec::new(),
            seg_dir: Vec::new(),
            node_coords: Vec::new(),
            nodes: HashMap::new(),
            cells: HashMap::new(),
        };
        for feature in &features {
            let dir = feature_dir(feature.get("properties"));
            let Some(geometry) = feature.get("geometry") else { continue };
            let kind = geometry.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match kind {
                "LineString" => {
                    if let Some(c) = geometry.get("coordinates") {
                        b.add_line(c, dir);
                    }
                }
                "MultiLineString" => {
                    for line in geometry
                        .get("coordinates")
                        .and_then(|c| c.as_array())
                        .into_iter()
                        .flatten()
                    {
                        b.add_line(line, dir);
                    }
                }
                _ => {}
            }
        }

        let segment_count = b.coords.len() as u32 / 4;
        if segment_count == 0 {
            return Err(StreetError(
                "no line features — expected LineString/MultiLineString geometries".into(),
            ));
        }

        // CSR adjacency: each segment contributes the edges its
        // direction allows — one-way streets enter the graph as
        // single-direction edges, so Dijkstra can only follow legal
        // travel.
        let node_count = b.node_coords.len() as u32 / 2;
        let mut degree = vec![0u32; node_count as usize];
        for i in 0..segment_count as usize {
            let dir = Dir::from_code(b.seg_dir[i]);
            if dir.allows_forward() {
                degree[b.seg_node[2 * i] as usize] += 1;
            }
            if dir.allows_backward() {
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
            let dir = Dir::from_code(b.seg_dir[i]);
            let a = b.seg_node[2 * i] as usize;
            let bn = b.seg_node[2 * i + 1] as usize;
            if dir.allows_forward() {
                let ca = cursor[a] as usize;
                adj_seg[ca] = i as u32;
                adj_node[ca] = b.seg_node[2 * i + 1];
                cursor[a] += 1;
            }
            if dir.allows_backward() {
                let cb = cursor[bn] as usize;
                adj_seg[cb] = i as u32;
                adj_node[cb] = b.seg_node[2 * i];
                cursor[bn] += 1;
            }
        }

        Ok(StreetNetwork {
            coords: b.coords,
            seg_len: b.seg_len,
            seg_node: b.seg_node,
            seg_dir: b.seg_dir,
            node_coords: b.node_coords,
            adj_offset,
            adj_seg,
            adj_node,
            cells: b.cells,
            segment_count,
            node_count,
        })
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
        let from_dir = Dir::from_code(self.seg_dir[from.seg as usize]);
        let to_dir = Dir::from_code(self.seg_dir[to.seg as usize]);

        // Same segment: the segment is straight — but only when the
        // direction allows reaching a target that sits behind the
        // position. On a one-way the route must leave and come back,
        // which is the Dijkstra below.
        if from.seg == to.seg {
            let reachable = match from_dir {
                Dir::Both => true,
                Dir::Forward => to.t >= from.t,
                Dir::Backward => to.t <= from.t,
            };
            if reachable {
                return Some(vec![StreetPoint { lat: to.lat, lon: to.lon }]);
            }
        }

        // The two ends of the target segment act as virtual edges to
        // the exact position: entering the segment at nodeA leaves
        // t*len to walk, at nodeB (1-t)*len. Entering is itself
        // travel — nodeA only when the segment runs forward, nodeB
        // only when it runs backward.
        let to_len = self.seg_len[to.seg as usize];
        let to_a = self.seg_node[2 * to.seg as usize];
        let to_b = self.seg_node[2 * to.seg as usize + 1];
        let inside_cost = |node: u32| -> Option<f64> {
            if node == to_a && to_dir.allows_forward() {
                Some(to.t * to_len)
            } else if node == to_b && to_dir.allows_backward() {
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
        // Leaving toward the last vertex is travel in the digitized
        // direction; toward the first vertex is travel against it.
        let mut starts: Vec<(f64, usize)> = Vec::new();
        if from_dir.allows_forward() {
            starts.push(((1.0 - from.t) * from_len, from_b));
        }
        if from_dir.allows_backward() {
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
        Some(points)
    }
}
