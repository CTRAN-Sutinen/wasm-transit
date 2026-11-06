//! Coverage against a WEIGHTED LAYER (D-47): what share of a layer's
//! weight — people, jobs, zero-car households, whatever the planner
//! imported and labelled — sits within a radius of a stop with
//! service, and of a stop with frequent service.
//!
//! **The layer is any weighted geography.** A GeoJSON
//! FeatureCollection of points and polygons; its numeric properties
//! are the weights it can be read by. The document is the bytes: which
//! property is the weight (`field`) and what it means (`label`) are
//! the question's, so one file carrying both people and jobs is one
//! layer read two ways.
//!
//! **Polygons are weighted by area.** A polygon's weight spreads
//! evenly over its area: it is sampled on a lattice at parse time
//! (each sample carrying an equal share, the shares summing to one),
//! and a polygon is served by the share of its samples within the
//! radius. A polygon wholly inside the radius is wholly served; one
//! straddling it is served by about the share of its area inside, to
//! the sampling resolution the answer states (`sample_spacing_m`).
//! **Points stay valid**: a point's whole weight is served or not, and
//! the answer states the layer's resolution as the average area per
//! point (`point_spacing_m`) — a tract centroid two kilometres from
//! the next swings with the radius in a way a block's does not.
//!
//! **Frequent counts departures (D-6).** Served is any visit (a rider
//! alights at a terminal); frequent is departures only, on the whole
//! day and in a window — a terminal where nobody boards is never
//! frequent.
//!
//! **Coverage answers a combination (D-55).** `type_coverage` and
//! `unserved_points` read the service types that run together, pooled
//! over their authored service with no date standing in; on every date
//! a combination holds exactly its members run, so its coverage is
//! each of those dates' ([`coverage`], the calendar view, is the
//! laws' oracle).
//!
//! **A comparison is churn between two named sides.** `coverage_compare`
//! pools the service types the host names on each side (D-99) and
//! answers the weight gained and lost; it never matches one plan's
//! types to the other's (D-43's matching rule is open).

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use serde::Serialize;
use ts_rs::TS;

use transit_network::Network;

/// The finest lattice a polygon is sampled on, in meters.
const MIN_SAMPLE_SPACING_M: f64 = 25.0;
/// The samples a polygon aims for at most: a big polygon's lattice
/// coarsens until its area holds about this many.
const TARGET_SAMPLES: f64 = 4096.0;
/// The lattice a polygon's bounding box may hold at most (a long,
/// thin, diagonal polygon's box is mostly outside it).
const MAX_LATTICE: f64 = 65_536.0;
/// Meters per degree of latitude (mean earth radius).
const M_PER_DEG: f64 = 6_371_000.0 * std::f64::consts::PI / 180.0;

// ---------------------------------------------------------------------
// The layer document
// ---------------------------------------------------------------------

/// A polygon as GeoJSON holds it: rings of [lon, lat], the first the
/// outer ring, the rest holes.
pub type Rings = Vec<Vec<(f64, f64)>>;

/// One feature's place.
#[derive(Debug, Clone)]
enum Place {
    Point { lat: f64, lon: f64 },
    /// A polygon or multipolygon: its parts, its samples (lat, lon,
    /// share of the feature — the shares sum to one), and the lattice
    /// spacing they were taken at.
    Area { parts: Vec<Rings>, samples: Vec<(f64, f64, f64)>, spacing_m: f64 },
}

#[derive(Debug, Clone)]
struct Feature {
    place: Place,
    /// Numeric properties: (name, value, whether the JSON value is a
    /// number — a numeric string is read only when named).
    values: Vec<(String, f64, bool)>,
}

impl Feature {
    fn value(&self, field: &str) -> Option<f64> {
        self.values.iter().find(|(name, ..)| name == field).map(|(_, v, _)| *v)
    }
}

/// An imported weighted layer: points and polygons with their numeric
/// properties. Registered by the kernel under the hash of its bytes.
#[derive(Debug, Clone)]
pub struct WeightedLayer {
    features: Vec<Feature>,
    fields: Vec<LayerField>,
    /// Features with no geometry coverage reads (lines, a missing
    /// geometry).
    skipped: u32,
}

/// One weight a layer can be read by: a property that is a JSON number
/// on at least one feature, and its total over the layer.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LayerField {
    pub name: String,
    pub total: f64,
}

/// How a coverage answer read its layer: the planner's label, the
/// weight field, what the layer holds, and its resolution.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LayerReading {
    /// What the weight means, as the planner named it (the field when
    /// they did not).
    pub label: String,
    pub field: String,
    /// The features carrying the field, and of those the points and
    /// the polygons.
    pub features: u32,
    pub points: u32,
    pub polygons: u32,
    /// The weight over those features.
    pub total: f64,
    /// A point layer's resolution: the side of the average area per
    /// point (the points' bounding box over their count), in meters.
    /// Null with fewer than two points.
    pub point_spacing_m: Option<f64>,
    /// A polygon layer's resolution: the coarsest lattice its polygons
    /// were sampled on, in meters. Null with no polygons.
    pub sample_spacing_m: Option<f64>,
}

/// A layer read by one weight: the samples coverage measures.
#[derive(Debug, Clone)]
pub struct Weights<'a> {
    pub reading: LayerReading,
    layer: &'a WeightedLayer,
    /// (lat, lon, weight, feature index).
    samples: Vec<(f64, f64, f64, u32)>,
}

impl WeightedLayer {
    /// The weights the layer can be read by, by name.
    pub fn fields(&self) -> &[LayerField] {
        &self.fields
    }

    pub fn features(&self) -> usize {
        self.features.len()
    }

    pub fn points(&self) -> usize {
        self.features.iter().filter(|f| matches!(f.place, Place::Point { .. })).count()
    }

    pub fn polygons(&self) -> usize {
        self.features.len() - self.points()
    }

    /// Read the layer by one weight. `field` names the property; with
    /// none, a layer with exactly one numeric property reads by it,
    /// and any other layer is an error naming its fields. `label` is
    /// what the weight means (the field when none). Features without
    /// the field are left out of the reading.
    pub fn weights(&self, field: Option<&str>, label: Option<&str>) -> Result<Weights<'_>, String> {
        let field = match field {
            Some(f) => f.to_string(),
            None => match self.fields.as_slice() {
                [one] => one.name.clone(),
                [] => return Err("layer: no numeric property to weigh by".into()),
                many => {
                    let names: Vec<&str> = many.iter().map(|f| f.name.as_str()).collect();
                    return Err(format!("layer: several numeric properties ({}); name the field", names.join(", ")));
                }
            },
        };
        let mut samples = Vec::new();
        let (mut features, mut points, mut polygons, mut total) = (0u32, 0u32, 0u32, 0.0f64);
        let mut sample_spacing: Option<f64> = None;
        let (mut lat_lo, mut lat_hi, mut lon_lo, mut lon_hi) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for (index, feature) in self.features.iter().enumerate() {
            let Some(value) = feature.value(&field) else { continue };
            features += 1;
            total += value;
            match &feature.place {
                Place::Point { lat, lon } => {
                    points += 1;
                    (lat_lo, lat_hi) = (lat_lo.min(*lat), lat_hi.max(*lat));
                    (lon_lo, lon_hi) = (lon_lo.min(*lon), lon_hi.max(*lon));
                    samples.push((*lat, *lon, value, index as u32));
                }
                Place::Area { samples: shares, spacing_m, .. } => {
                    polygons += 1;
                    sample_spacing = Some(sample_spacing.map_or(*spacing_m, |s| s.max(*spacing_m)));
                    samples.extend(shares.iter().map(|(lat, lon, share)| (*lat, *lon, value * share, index as u32)));
                }
            }
        }
        if features == 0 {
            return Err(format!("layer: no feature carries a numeric {field:?}"));
        }
        let point_spacing_m = (points >= 2).then(|| {
            let mid = ((lat_lo + lat_hi) / 2.0).to_radians().cos();
            let area = ((lat_hi - lat_lo) * M_PER_DEG) * ((lon_hi - lon_lo) * M_PER_DEG * mid);
            (area / points as f64).sqrt()
        });
        Ok(Weights {
            reading: LayerReading {
                label: label.map(str::to_string).unwrap_or_else(|| field.clone()),
                field,
                features,
                points,
                polygons,
                total,
                point_spacing_m,
                sample_spacing_m: sample_spacing,
            },
            layer: self,
            samples,
        })
    }
}

/// Parse a weighted layer from GeoJSON: a FeatureCollection of Point,
/// Polygon and MultiPolygon features, each keeping its numeric
/// properties (JSON numbers, and numeric strings when a reading names
/// them). Other geometries are skipped and counted.
pub fn parse_layer(bytes: &[u8]) -> Result<WeightedLayer, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| format!("layer: {e}"))?;
    let raw = value
        .get("features")
        .and_then(|f| f.as_array())
        .ok_or("layer: not a FeatureCollection (no features array)")?;
    let mut features = Vec::with_capacity(raw.len());
    let mut skipped = 0u32;
    let mut totals: BTreeMap<String, f64> = BTreeMap::new();
    for feature in raw {
        let Some(place) = feature.get("geometry").and_then(place_of) else {
            skipped += 1;
            continue;
        };
        let mut values = Vec::new();
        if let Some(props) = feature.get("properties").and_then(|p| p.as_object()) {
            for (name, v) in props {
                if let Some(n) = v.as_f64() {
                    *totals.entry(name.clone()).or_default() += n;
                    values.push((name.clone(), n, true));
                } else if let Some(n) = v.as_str().and_then(|s| s.trim().parse::<f64>().ok()).filter(|n| n.is_finite()) {
                    values.push((name.clone(), n, false));
                }
            }
        }
        features.push(Feature { place, values });
    }
    if features.is_empty() {
        return Err("layer: no point or polygon features".into());
    }
    let fields = totals.into_iter().map(|(name, total)| LayerField { name, total }).collect();
    Ok(WeightedLayer { features, fields, skipped })
}

impl WeightedLayer {
    /// Features with a geometry coverage does not read.
    pub fn skipped(&self) -> u32 {
        self.skipped
    }
}

fn coord(v: &serde_json::Value) -> Option<(f64, f64)> {
    let c = v.as_array()?;
    let (lon, lat) = (c.first()?.as_f64()?, c.get(1)?.as_f64()?);
    (lon.is_finite() && lat.is_finite()).then_some((lon, lat))
}

fn rings(v: &serde_json::Value) -> Option<Rings> {
    let rings: Rings = v
        .as_array()?
        .iter()
        .map(|ring| ring.as_array().map(|pts| pts.iter().filter_map(coord).collect::<Vec<_>>()))
        .collect::<Option<_>>()?;
    (rings.first().is_some_and(|outer| outer.len() >= 3)).then_some(rings)
}

fn place_of(geometry: &serde_json::Value) -> Option<Place> {
    let coords = geometry.get("coordinates")?;
    let parts = match geometry.get("type")?.as_str()? {
        "Point" => {
            let (lon, lat) = coord(coords)?;
            return Some(Place::Point { lat, lon });
        }
        "Polygon" => vec![rings(coords)?],
        "MultiPolygon" => coords.as_array()?.iter().filter_map(rings).collect(),
        _ => return None,
    };
    if parts.is_empty() {
        return None;
    }
    let (samples, spacing_m) = sample(&parts);
    Some(Place::Area { parts, samples, spacing_m })
}

/// Even-odd point-in-polygon over one polygon's rings (holes included).
fn inside(rings: &[Vec<(f64, f64)>], x: f64, y: f64, project: &impl Fn((f64, f64)) -> (f64, f64)) -> bool {
    let mut odd = false;
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let (xi, yi) = project(ring[i]);
            let (xj, yj) = project(ring[(i + n - 1) % n]);
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                odd = !odd;
            }
        }
    }
    odd
}

/// The polygon's samples: lattice cell centres inside it, each an
/// equal share (summing to one), and the lattice spacing. A polygon
/// smaller than one cell is one sample at its outer ring's mean.
fn sample(parts: &[Rings]) -> (Vec<(f64, f64, f64)>, f64) {
    let all = || parts.iter().flat_map(|rings| rings.iter().flatten());
    let (mut lon_lo, mut lon_hi, mut lat_lo, mut lat_hi) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &(lon, lat) in all() {
        (lon_lo, lon_hi, lat_lo, lat_hi) = (lon_lo.min(lon), lon_hi.max(lon), lat_lo.min(lat), lat_hi.max(lat));
    }
    // A local equirectangular frame in meters around the box's centre.
    let (lat0, lon0) = ((lat_lo + lat_hi) / 2.0, (lon_lo + lon_hi) / 2.0);
    let kx = M_PER_DEG * lat0.to_radians().cos();
    let project = |(lon, lat): (f64, f64)| ((lon - lon0) * kx, (lat - lat0) * M_PER_DEG);
    let unproject = |x: f64, y: f64| (lat0 + y / M_PER_DEG, lon0 + x / kx);
    let area: f64 = parts
        .iter()
        .map(|rings| {
            let shoelace = |ring: &Vec<(f64, f64)>| {
                let n = ring.len();
                (0..n)
                    .map(|i| {
                        let (x1, y1) = project(ring[i]);
                        let (x2, y2) = project(ring[(i + 1) % n]);
                        x1 * y2 - x2 * y1
                    })
                    .sum::<f64>()
                    .abs()
                    / 2.0
            };
            let outer = shoelace(&rings[0]);
            (outer - rings[1..].iter().map(shoelace).sum::<f64>()).max(0.0)
        })
        .sum();
    let (w, h) = ((lon_hi - lon_lo) * kx, (lat_hi - lat_lo) * M_PER_DEG);
    let spacing = MIN_SAMPLE_SPACING_M.max((area / TARGET_SAMPLES).sqrt()).max((w * h / MAX_LATTICE).sqrt());
    let (x_lo, y_lo) = project((lon_lo, lat_lo));
    let (nx, ny) = ((w / spacing).ceil().max(1.0) as usize, (h / spacing).ceil().max(1.0) as usize);
    let mut points = Vec::new();
    for j in 0..ny {
        let y = y_lo + (j as f64 + 0.5) * spacing;
        for i in 0..nx {
            let x = x_lo + (i as f64 + 0.5) * spacing;
            if parts.iter().any(|rings| inside(rings, x, y, &project)) {
                points.push(unproject(x, y));
            }
        }
    }
    if points.is_empty() {
        let outer = &parts[0][0];
        let n = outer.len() as f64;
        let (lon, lat) = outer.iter().fold((0.0, 0.0), |(a, b), (lon, lat)| (a + lon, b + lat));
        points.push((lat / n, lon / n));
    }
    let share = 1.0 / points.len() as f64;
    (points.into_iter().map(|(lat, lon)| (lat, lon, share)).collect(), spacing)
}

// ---------------------------------------------------------------------
// What a date serves
// ---------------------------------------------------------------------

/// "Frequent" inside a time window — the way planners state it:
/// "every 15 minutes, 07:00–19:00". A stop is frequent in the window
/// when no rider waits longer than `frequent_secs` anywhere in it:
/// the gap from the window's start to the first departure, every gap
/// between departures, and the gap from the last departure to the
/// window's end all fit. Departures only (a trip's final visit is an
/// arrival); every route at the stop pools, as a rider sees it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct CoverageWindow {
    pub start_secs: u32,
    pub end_secs: u32,
}

impl CoverageWindow {
    /// The longest wait inside the window over sorted departures; None
    /// when nothing departs inside it.
    fn worst_wait_secs(&self, sorted: &[u32]) -> Option<u32> {
        let inside: Vec<u32> = sorted.iter().copied().filter(|t| (self.start_secs..=self.end_secs).contains(t)).collect();
        let first = *inside.first()?;
        let last = *inside.last()?;
        let between = inside.windows(2).map(|w| w[1] - w[0]).max().unwrap_or(0);
        Some((first - self.start_secs).max(between).max(self.end_secs - last))
    }
}

/// The stops some service serves (any visit) and the frequent ones
/// (departures only), with their coordinates, in id order.
pub(crate) struct Served {
    served: Vec<(String, f64, f64)>,
    frequent: Vec<(f64, f64)>,
}

/// What runs on a date.
fn on_date(net: &Network, date: NaiveDate) -> impl Fn(&str) -> bool + '_ {
    move |t: &str| net.service_types.get(t).is_some_and(|st| st.rule.runs_on(date))
}

/// What the `members` run.
fn of_members(members: &[String]) -> impl Fn(&str) -> bool + '_ {
    move |t: &str| members.iter().any(|m| m == t)
}

/// The stops the service types `include` admits serve, pooled over
/// their authored service.
fn served_by(
    net: &Network,
    include: impl Fn(&str) -> bool,
    frequent_secs: u32,
    min_departures: u32,
    window: Option<CoverageWindow>,
) -> Served {
    // Visits and departure times per stop — every service and route
    // calling at the stop pools: two routes at 30 minutes each make
    // one 15-minute stop.
    let mut visited: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for (service_id, service) in &net.pattern_trips {
        if !include(&service.service_type) {
            continue;
        }
        let Ok(materialized) = transit_network::materialize_service(net, service_id) else {
            continue;
        };
        for trip in &materialized {
            let last = trip.stop_times.len().saturating_sub(1);
            for (i, st) in trip.stop_times.iter().enumerate() {
                let departs = visited.entry(st.stop.clone()).or_default();
                if i < last {
                    departs.push(st.departure_secs);
                }
            }
        }
    }
    let mut out = Served { served: Vec::new(), frequent: Vec::new() };
    for (stop_id, departs) in &mut visited {
        let Some(stop) = net.stops.get(stop_id) else { continue };
        out.served.push((stop_id.clone(), stop.lat, stop.lon));
        departs.sort_unstable();
        let n = departs.len() as u32;
        let is_frequent = match window {
            // Inside the window: the worst wait fits, with enough
            // departures that a lone pair cannot pass.
            Some(window) => {
                let inside = departs.iter().filter(|t| (window.start_secs..=window.end_secs).contains(t)).count();
                inside as u32 >= min_departures && window.worst_wait_secs(departs).is_some_and(|w| w <= frequent_secs)
            }
            // The whole day: the average headway over the departures.
            None => {
                n >= min_departures.max(2) && (departs[departs.len() - 1] - departs[0]) / (n - 1) <= frequent_secs
            }
        };
        if is_frequent {
            out.frequent.push((stop.lat, stop.lon));
        }
    }
    out
}

/// Great-circle distance in meters (haversine, mean earth radius).
pub(crate) fn distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dlat = p2 - p1;
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dlon / 2.0).sin().powi(2);
    6_371_000.0 * 2.0 * a.sqrt().min(1.0).asin()
}

/// Stops bucketed on a grid at least a radius wide, so "is any stop
/// within the radius" reads the 3×3 cells around a sample.
struct Grid {
    radius_m: f64,
    cell_lat: f64,
    cell_lon: f64,
    cells: HashMap<(i64, i64), Vec<(f64, f64)>>,
}

impl Grid {
    fn new(stops: impl Iterator<Item = (f64, f64)>, radius_m: f64) -> Grid {
        let stops: Vec<(f64, f64)> = stops.collect();
        let max_lat = stops.iter().map(|(lat, _)| lat.abs()).fold(0.0f64, f64::max);
        // Cells no smaller than the radius anywhere among the stops (a
        // degree of longitude shrinks toward the poles; a margin for
        // the haversine against the flat bound).
        let cell_lat = (radius_m.max(1.0) / (M_PER_DEG * 0.99)).max(1e-6);
        let cos = (max_lat + 1.0).min(89.0).to_radians().cos();
        let cell_lon = (radius_m.max(1.0) / (M_PER_DEG * cos * 0.99)).max(1e-6);
        let mut cells: HashMap<(i64, i64), Vec<(f64, f64)>> = HashMap::new();
        for (lat, lon) in stops {
            cells.entry(((lat / cell_lat).floor() as i64, (lon / cell_lon).floor() as i64)).or_default().push((lat, lon));
        }
        Grid { radius_m, cell_lat, cell_lon, cells }
    }

    fn near(&self, lat: f64, lon: f64) -> bool {
        let (ci, cj) = ((lat / self.cell_lat).floor() as i64, (lon / self.cell_lon).floor() as i64);
        (-1..=1).any(|di| {
            (-1..=1).any(|dj| {
                self.cells
                    .get(&(ci + di, cj + dj))
                    .is_some_and(|stops| stops.iter().any(|(s_lat, s_lon)| distance_m(lat, lon, *s_lat, *s_lon) <= self.radius_m))
            })
        })
    }
}

/// Each sample: served, and frequent (frequent implies served).
fn classify(weights: &Weights, radius_m: f64, served: &Served) -> Vec<(bool, bool)> {
    let served_grid = Grid::new(served.served.iter().map(|(_, lat, lon)| (*lat, *lon)), radius_m);
    let frequent_grid = Grid::new(served.frequent.iter().copied(), radius_m);
    weights
        .samples
        .iter()
        .map(|(lat, lon, ..)| {
            let s = served_grid.near(*lat, *lon);
            (s, s && frequent_grid.near(*lat, *lon))
        })
        .collect()
}

// ---------------------------------------------------------------------
// The answers
// ---------------------------------------------------------------------

/// Coverage on one service date: what share of the layer's weight is
/// within `radius_m` of a stop with service, and what share is near
/// *frequent* service (headway at or under `frequent_secs` over at
/// least `min_departures` departures, so a 2-trip pocket cannot read
/// as frequent).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CoverageStats {
    /// The service date measured; null for a combination's pooled
    /// service.
    pub date: Option<String>,
    pub layer: LayerReading,
    pub radius_m: f64,
    pub frequent_secs: u32,
    pub min_departures: u32,
    /// The window frequency was judged in; null = the whole-day
    /// average headway.
    pub window: Option<CoverageWindow>,
    pub total: f64,
    pub served: f64,
    pub frequent: f64,
    pub served_pct: f64,
    pub frequent_pct: f64,
    pub served_stops: u32,
    pub frequent_stops: u32,
}

/// Coverage on one service date: everything that runs that day. The
/// calendar view the combination reads ([`crate::type_coverage`]) agree
/// with on every date they hold.
pub fn coverage(
    net: &Network,
    date: NaiveDate,
    weights: &Weights,
    radius_m: f64,
    frequent_secs: u32,
    min_departures: u32,
    window: Option<CoverageWindow>,
) -> CoverageStats {
    let served = served_by(net, on_date(net, date), frequent_secs, min_departures, window);
    stats(Some(date), weights, radius_m, frequent_secs, min_departures, window, &served)
}

/// Coverage of the members' pooled authored service, on no date.
pub(crate) fn coverage_of_members(
    net: &Network,
    members: &[String],
    weights: &Weights,
    radius_m: f64,
    frequent_secs: u32,
    min_departures: u32,
    window: Option<CoverageWindow>,
) -> CoverageStats {
    let served = served_by(net, of_members(members), frequent_secs, min_departures, window);
    stats(None, weights, radius_m, frequent_secs, min_departures, window, &served)
}

fn stats(
    date: Option<NaiveDate>,
    weights: &Weights,
    radius_m: f64,
    frequent_secs: u32,
    min_departures: u32,
    window: Option<CoverageWindow>,
    served: &Served,
) -> CoverageStats {
    let (served_w, frequent_w) = weigh(weights, &classify(weights, radius_m, served));
    let total = weights.reading.total;
    CoverageStats {
        date: date.map(|d| d.format("%Y-%m-%d").to_string()),
        layer: weights.reading.clone(),
        radius_m,
        frequent_secs,
        min_departures,
        window,
        total,
        served: served_w,
        frequent: frequent_w,
        served_pct: pct(served_w, total),
        frequent_pct: pct(frequent_w, total),
        served_stops: served.served.len() as u32,
        frequent_stops: served.frequent.len() as u32,
    }
}

fn pct(part: f64, total: f64) -> f64 {
    if total > 0.0 {
        100.0 * part / total
    } else {
        0.0
    }
}

fn weigh(weights: &Weights, flags: &[(bool, bool)]) -> (f64, f64) {
    weights.samples.iter().zip(flags).fold((0.0, 0.0), |(s, f), ((.., w, _), (is_s, is_f))| {
        (s + if *is_s { *w } else { 0.0 }, f + if *is_f { *w } else { 0.0 })
    })
}

/// One feature nothing serves (wholly or in part) — the
/// WHERE of the unserved share ("which part, and where?"): its
/// unserved weight, where that part sits, and the nearest served stop.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UnservedPoint {
    /// A point's own position; for a polygon, the centre of its
    /// unserved part.
    pub lat: f64,
    pub lon: f64,
    /// The unserved weight.
    pub weight: f64,
    /// The feature's share unserved: 1 for a point, the unserved share
    /// of its area for a polygon.
    pub share: f64,
    /// The feature's index in the layer.
    pub feature: u32,
    /// The served stop nearest (lat, lon), and the distance to it.
    pub nearest_stop: Option<String>,
    pub nearest_m: f64,
    /// A polygon's parts (GeoJSON MultiPolygon coordinates, [lon,
    /// lat]); null for a point.
    pub polygon: Option<Vec<Rings>>,
}

/// A combination's unserved places: what its stops leave out.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UnservedPlaces {
    /// The combination's name: its members joined " + ".
    pub group: String,
    /// The service types that run together, sorted.
    pub members: Vec<String>,
    /// Heaviest unserved weight first.
    pub points: Vec<UnservedPoint>,
}

/// The unserved features of a layer under a service COMBINATION (D-55):
/// every feature with weight beyond `radius_m` of every stop its
/// members' pooled service calls at (the same served set
/// [`crate::type_coverage`] measures against — on any date it holds,
/// that day's), sorted by unserved weight descending then by position —
/// the biggest unserved places first. Served plus unserved conserves
/// the layer's total. None when the members never run together.
pub fn unserved_points(net: &Network, weights: &Weights, members: &[String], radius_m: f64) -> Option<UnservedPlaces> {
    let combination = transit_network::service_combination(net, members)?;
    let points = unserved_of(net, weights, &combination.members, radius_m);
    Some(UnservedPlaces { group: combination.id, members: combination.members, points })
}

fn unserved_of(net: &Network, weights: &Weights, members: &[String], radius_m: f64) -> Vec<UnservedPoint> {
    let served = served_by(net, of_members(members), u32::MAX, u32::MAX, None);
    let flags = classify(weights, radius_m, &served);
    // Per feature: the unserved weight, its weighted centre, and the
    // unserved sample count.
    let mut unserved: BTreeMap<u32, (f64, f64, f64, u32)> = BTreeMap::new();
    for ((lat, lon, w, feature), (is_served, _)) in weights.samples.iter().zip(&flags) {
        if *is_served {
            continue;
        }
        let e = unserved.entry(*feature).or_default();
        e.0 += w;
        e.1 += lat;
        e.2 += lon;
        e.3 += 1;
    }
    let total_samples: BTreeMap<u32, u32> = weights.samples.iter().fold(BTreeMap::new(), |mut m, s| {
        *m.entry(s.3).or_default() += 1;
        m
    });
    let mut out: Vec<UnservedPoint> = unserved
        .into_iter()
        .map(|(feature, (weight, lat_sum, lon_sum, n))| {
            let (lat, lon) = (lat_sum / n as f64, lon_sum / n as f64);
            // Served means within radius of SOME served stop; id order
            // breaks a distance tie toward the lowest id.
            let nearest = served.served.iter().fold(None::<(&str, f64)>, |best, (id, s_lat, s_lon)| {
                let d = distance_m(lat, lon, *s_lat, *s_lon);
                if best.map_or(true, |(_, bd)| d < bd) {
                    Some((id, d))
                } else {
                    best
                }
            });
            let place = &weights.layer.features[feature as usize].place;
            UnservedPoint {
                lat,
                lon,
                weight,
                share: n as f64 / total_samples[&feature] as f64,
                feature,
                nearest_stop: nearest.map(|(id, _)| id.to_string()),
                nearest_m: nearest.map_or(0.0, |(_, d)| d),
                polygon: match place {
                    Place::Area { parts, .. } => Some(parts.clone()),
                    Place::Point { .. } => None,
                },
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.weight
            .total_cmp(&a.weight)
            .then_with(|| a.lat.total_cmp(&b.lat))
            .then_with(|| a.lon.total_cmp(&b.lon))
    });
    out
}

/// One side of a comparison: the service types it pools, and what
/// they serve.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CoverageSide {
    /// The service types pooled on this side, as the host named them.
    pub members: Vec<String>,
    pub served: f64,
    pub frequent: f64,
    pub served_pct: f64,
    pub frequent_pct: f64,
    pub served_stops: u32,
    pub frequent_stops: u32,
}

/// A proposal's coverage against its base: both sides, and the churn
/// a net change hides — the weight the proposal newly serves
/// (`served_gained`) and no longer serves (`served_lost`), and the
/// same for frequent service. The proposal's served weight is the
/// base's plus gained minus lost.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CoverageDelta {
    pub base: CoverageSide,
    pub proposal: CoverageSide,
    pub served_gained: f64,
    pub served_lost: f64,
    pub frequent_gained: f64,
    pub frequent_lost: f64,
}

/// Compare two networks' coverage of one layer, each side the
/// service types the host names: `base_members` of the base,
/// `members` of the proposal, each pooled over its authored service
/// (a combination of types that run together, D-55). Which of one
/// plan's combinations answers to which of the other's — when a fork
/// renames or splits a type — is the host's to say here; the op never
/// matches them (the rule is not decided, QAF-20).
#[allow(clippy::too_many_arguments)]
pub fn coverage_compare(
    base: &Network,
    proposal: &Network,
    weights: &Weights,
    base_members: &[String],
    members: &[String],
    radius_m: f64,
    frequent_secs: u32,
    min_departures: u32,
    window: Option<CoverageWindow>,
) -> CoverageDelta {
    let total = weights.reading.total;
    let side = |net: &Network, members: &[String]| {
        let served = served_by(net, of_members(members), frequent_secs, min_departures, window);
        let flags = classify(weights, radius_m, &served);
        let (s, f) = weigh(weights, &flags);
        let side = CoverageSide {
            members: members.to_vec(),
            served: s,
            frequent: f,
            served_pct: pct(s, total),
            frequent_pct: pct(f, total),
            served_stops: served.served.len() as u32,
            frequent_stops: served.frequent.len() as u32,
        };
        (side, flags)
    };
    let (base_side, before) = side(base, base_members);
    let (proposal_side, after) = side(proposal, members);
    let mut delta = [0.0f64; 4];
    for ((.., w, _), ((s0, f0), (s1, f1))) in weights.samples.iter().zip(before.iter().zip(&after)) {
        if *s1 && !*s0 {
            delta[0] += w;
        }
        if *s0 && !*s1 {
            delta[1] += w;
        }
        if *f1 && !*f0 {
            delta[2] += w;
        }
        if *f0 && !*f1 {
            delta[3] += w;
        }
    }
    CoverageDelta {
        base: base_side,
        proposal: proposal_side,
        served_gained: delta[0],
        served_lost: delta[1],
        frequent_gained: delta[2],
        frequent_lost: delta[3],
    }
}
