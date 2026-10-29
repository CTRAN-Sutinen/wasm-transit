//! The streets import laws: a street layer (GeoJSON lines) in, the
//! canonical streets document out.
//!
//! The intent they encode (docs/units/streets-model.md):
//!
//! * **Streets are a model, not a file.** The document's identity is
//!   the streets, so what does not change the streets does not change
//!   the document: the layer's feature order, the direction a two-way
//!   street was digitized in (a one-way's travel direction is kept,
//!   whichever way it was drawn), the same street drawn twice, and
//!   every property outside the model (OBJECTID, publish dates).
//! * **Only streets are streets** (D-30, encoded provisionally): a
//!   line whose class field names something other than a road
//!   (railroad, water, a boundary, a power line) is not in the
//!   document; a line with no class field, or a road class the import
//!   does not know, is a street of class `unknown`. The class mapping
//!   is D-30's table, written out here independently.
//! * **The geometry is the layer's**, at the model's 1e-6 degree
//!   resolution: a malformed vertex breaks the line (a gap, never a
//!   segment), repeated vertices collapse, and a run of fewer than two
//!   distinct vertices is no street.
//! * **The document is valid by construction**, and a layer with no
//!   street is an error, not an empty document.
//!
//! The oracle rebuilds the expected document from the generated spec
//! with its own code; the canonical order it sorts by is the
//! documented one (points, then one-way, then class by wire name,
//! then name, an absent name first).

use proptest::collection::vec;
use proptest::prelude::*;
use serde_json::{json, Value};
use transit_streets::geojson::import;
use transit_streets::Streets;

const LAT0: f64 = 45.6;
const LON0: f64 = -122.6;
const STEP: f64 = 0.001;

// ---------------------------------------------------------------------
// The spec a layer is generated from
// ---------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Dir {
    Both,
    Forward,
    Backward,
}

/// How a feature spells its one-way (or does not).
#[derive(Clone, Copy, Debug)]
enum OneWaySpelling {
    Tiger, // OneWay ">" / "<"
    Osm,   // oneway "yes" / "-1"
}

#[derive(Clone, Debug)]
enum ClassField {
    None,
    Cfcc(Option<&'static str>),
    Mtfcc(&'static str),
    Highway(&'static str),
    Other(&'static str, &'static str),
}

#[derive(Clone, Debug)]
enum NameField {
    None,
    Full(String),
    Osm(String),
    Part(String),
    FullAndPart(String, String),
    Empty,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Vertex {
    At(i64, i64),
    Malformed(u8),
}

#[derive(Clone, Debug)]
struct Feature {
    lines: Vec<Vec<Vertex>>,
    dir: Dir,
    spelling: OneWaySpelling,
    class: ClassField,
    name: NameField,
    object_id: u32,
}

const CFCC: &[Option<&str>] = &[
    Some("A10"), Some("A15"), Some("A20"), Some("A30"), Some("A40"), Some("A41"),
    Some("A50"), Some("A60"), Some("A61"), Some("A62"), Some("A63"), Some("A64"),
    Some("A65"), Some("A70"), Some("A71"), Some("A72"), Some("A73"), Some("A74"),
    Some("A75"), Some("a40"), Some("B10"), Some("B20"), Some("C30"), Some("F10"),
    Some("H10"), Some("H20"), Some("H30"), Some("X10"), None,
];
const MTFCC: &[&str] = &[
    "S1100", "S1200", "S1400", "S1500", "S1630", "S1640", "S1710", "S1720", "S1730",
    "S1740", "S1750", "S1780", "S1820", "S1830", "R1011", "H3010", "P0001",
];
const HIGHWAY: &[&str] = &[
    "motorway", "trunk", "primary", "secondary", "tertiary", "residential",
    "unclassified", "living_street", "road", "motorway_link", "trunk_link",
    "primary_link", "secondary_link", "tertiary_link", "service", "track", "footway",
    "path", "pedestrian", "cycleway", "bridleway", "steps", "construction", "proposed",
];
const OTHER: &[(&str, &str)] = &[
    ("railway", "rail"),
    ("waterway", "river"),
    ("natural", "coastline"),
    ("power", "line"),
    ("boundary", "administrative"),
];
const NAMES: &[&str] = &["NE 21ST AVE", "Main St", "UNNAMED", "Fourth Plain Blvd"];

fn class_field() -> impl Strategy<Value = ClassField> {
    prop_oneof![
        Just(ClassField::None),
        prop::sample::select(CFCC).prop_map(ClassField::Cfcc),
        prop::sample::select(MTFCC).prop_map(ClassField::Mtfcc),
        prop::sample::select(HIGHWAY).prop_map(ClassField::Highway),
        prop::sample::select(OTHER).prop_map(|(k, v)| ClassField::Other(k, v)),
    ]
}

fn name_field() -> impl Strategy<Value = NameField> {
    let name = || prop::sample::select(NAMES).prop_map(str::to_string);
    prop_oneof![
        Just(NameField::None),
        name().prop_map(NameField::Full),
        name().prop_map(NameField::Osm),
        name().prop_map(NameField::Part),
        (name(), name()).prop_map(|(f, p)| NameField::FullAndPart(f, p)),
        Just(NameField::Empty),
    ]
}

fn vertex() -> impl Strategy<Value = Vertex> {
    prop_oneof![
        12 => (0i64..6, 0i64..6).prop_map(|(a, b)| Vertex::At(a, b)),
        1 => (0u8..3).prop_map(Vertex::Malformed),
    ]
}

fn feature() -> impl Strategy<Value = Feature> {
    (
        vec(vec(vertex(), 1..=6), 1..=3),
        prop_oneof![Just(Dir::Both), Just(Dir::Forward), Just(Dir::Backward)],
        prop_oneof![Just(OneWaySpelling::Tiger), Just(OneWaySpelling::Osm)],
        class_field(),
        name_field(),
        any::<u32>(),
    )
        .prop_map(|(lines, dir, spelling, class, name, object_id)| Feature {
            lines,
            dir,
            spelling,
            class,
            name,
            object_id,
        })
}

fn layer() -> impl Strategy<Value = Vec<Feature>> {
    vec(feature(), 1..=8)
}

// ---------------------------------------------------------------------
// The layer's GeoJSON
// ---------------------------------------------------------------------

fn lat(v: i64) -> f64 {
    LAT0 + v as f64 * STEP
}

fn lon(v: i64) -> f64 {
    LON0 + v as f64 * STEP
}

fn vertex_json(v: &Vertex) -> Value {
    match v {
        Vertex::At(a, b) => json!([lon(*b), lat(*a)]),
        Vertex::Malformed(0) => Value::Null,
        Vertex::Malformed(1) => json!(["x", 1.0]),
        Vertex::Malformed(_) => json!([lon(0)]),
    }
}

/// The feature as GeoJSON. `junk` adds properties outside the model.
fn feature_json(f: &Feature, junk: bool) -> Value {
    let mut props = serde_json::Map::new();
    match (f.dir, f.spelling) {
        (Dir::Both, _) => {}
        (Dir::Forward, OneWaySpelling::Tiger) => drop(props.insert("OneWay".into(), json!(">"))),
        (Dir::Backward, OneWaySpelling::Tiger) => drop(props.insert("OneWay".into(), json!("<"))),
        (Dir::Forward, OneWaySpelling::Osm) => drop(props.insert("oneway".into(), json!("yes"))),
        (Dir::Backward, OneWaySpelling::Osm) => drop(props.insert("oneway".into(), json!("-1"))),
    }
    match &f.class {
        ClassField::None => {}
        ClassField::Cfcc(code) => drop(props.insert("CFCC".into(), json!(code))),
        ClassField::Mtfcc(code) => drop(props.insert("MTFCC".into(), json!(code))),
        ClassField::Highway(v) => drop(props.insert("highway".into(), json!(v))),
        ClassField::Other(k, v) => drop(props.insert((*k).into(), json!(v))),
    }
    match &f.name {
        NameField::None => {}
        NameField::Full(n) => drop(props.insert("FULLNAME".into(), json!(n))),
        NameField::Osm(n) => drop(props.insert("name".into(), json!(n))),
        NameField::Part(n) => drop(props.insert("NAME".into(), json!(n))),
        NameField::FullAndPart(full, part) => {
            props.insert("FULLNAME".into(), json!(full));
            props.insert("NAME".into(), json!(part));
        }
        NameField::Empty => drop(props.insert("FULLNAME".into(), json!(""))),
    }
    if junk {
        props.insert("OBJECTID".into(), json!(f.object_id));
        props.insert("PublishDt".into(), json!("2026-10-29"));
        props.insert("F_ZLEVEL".into(), json!(0));
    }
    let geometry = if f.lines.len() == 1 {
        json!({ "type": "LineString", "coordinates": f.lines[0].iter().map(vertex_json).collect::<Vec<_>>() })
    } else {
        json!({
            "type": "MultiLineString",
            "coordinates": f.lines.iter().map(|l| l.iter().map(vertex_json).collect::<Vec<_>>()).collect::<Vec<_>>(),
        })
    };
    json!({ "type": "Feature", "properties": props, "geometry": geometry })
}

fn layer_bytes(features: &[Feature], junk: bool) -> Vec<u8> {
    let fc = json!({
        "type": "FeatureCollection",
        "features": features.iter().map(|f| feature_json(f, junk)).collect::<Vec<_>>(),
    });
    serde_json::to_vec(&fc).unwrap()
}

// ---------------------------------------------------------------------
// The oracle
// ---------------------------------------------------------------------

/// D-30's table. Outer None: not a street.
fn expected_class(field: &ClassField) -> Option<&'static str> {
    match field {
        ClassField::None => Some("unknown"),
        ClassField::Cfcc(None) => Some("unknown"),
        ClassField::Cfcc(Some(code)) => {
            let code = code.to_ascii_uppercase();
            let letter = &code[..1];
            if ["B", "C", "D", "E", "F", "H"].contains(&letter) {
                return None;
            }
            if letter != "A" {
                return Some("unknown");
            }
            Some(match &code[..2] {
                "A1" => "motorway",
                "A2" => "primary",
                "A3" => "secondary",
                "A4" => "local",
                "A5" => "track",
                _ => match code.as_str() {
                    "A60" | "A61" | "A62" => "local",
                    "A63" => "ramp",
                    "A64" | "A73" | "A74" => "service",
                    "A70" | "A71" => "path",
                    "A72" => "steps",
                    _ => "unknown",
                },
            })
        }
        ClassField::Mtfcc(code) => {
            if !code.starts_with("S1") {
                return None;
            }
            Some(match *code {
                "S1100" => "motorway",
                "S1200" => "primary",
                "S1400" => "local",
                "S1500" => "track",
                "S1630" => "ramp",
                "S1640" | "S1730" | "S1740" | "S1780" => "service",
                "S1710" | "S1820" | "S1830" => "path",
                "S1720" => "steps",
                _ => "unknown",
            })
        }
        ClassField::Highway(v) => Some(match *v {
            "motorway" => "motorway",
            "trunk" | "primary" => "primary",
            "secondary" | "tertiary" => "secondary",
            "residential" | "unclassified" | "living_street" | "road" => "local",
            "service" => "service",
            "track" => "track",
            "footway" | "path" | "pedestrian" | "cycleway" | "bridleway" => "path",
            "steps" => "steps",
            v if v.ends_with("_link") => "ramp",
            _ => "unknown",
        }),
        ClassField::Other(..) => None,
    }
}

fn expected_name(field: &NameField) -> Option<String> {
    match field {
        NameField::None | NameField::Empty => None,
        NameField::Full(n) | NameField::Osm(n) | NameField::Part(n) => Some(n.clone()),
        NameField::FullAndPart(full, _) => Some(full.clone()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OWay {
    points: Vec<[i32; 2]>,
    oneway: bool,
    class: String,
    name: Option<String>,
}

fn e6(x: f64) -> i32 {
    (x * 1e6).round() as i32
}

fn expected_ways(features: &[Feature]) -> Vec<OWay> {
    let mut ways = Vec::new();
    for f in features {
        let Some(class) = expected_class(&f.class) else { continue };
        let name = expected_name(&f.name);
        for line in &f.lines {
            // Malformed vertices break the line into runs.
            let mut runs: Vec<Vec<[i32; 2]>> = vec![Vec::new()];
            for v in line {
                match v {
                    Vertex::At(a, b) => {
                        let p = [e6(lat(*a)), e6(lon(*b))];
                        let run = runs.last_mut().unwrap();
                        if run.last() != Some(&p) {
                            run.push(p);
                        }
                    }
                    Vertex::Malformed(_) => runs.push(Vec::new()),
                }
            }
            for mut points in runs.into_iter().filter(|r| r.len() >= 2) {
                let oneway = match f.dir {
                    Dir::Both => {
                        let reversed: Vec<_> = points.iter().rev().copied().collect();
                        if reversed < points {
                            points = reversed;
                        }
                        false
                    }
                    Dir::Forward => true,
                    Dir::Backward => {
                        points.reverse();
                        true
                    }
                };
                ways.push(OWay { points, oneway, class: class.to_string(), name: name.clone() });
            }
        }
    }
    ways.sort();
    ways.dedup();
    ways
}

fn actual_ways(doc: &Streets) -> Vec<OWay> {
    doc.ways
        .iter()
        .map(|w| OWay {
            points: w.points.clone(),
            oneway: w.oneway,
            class: serde_json::to_value(w.class).unwrap().as_str().unwrap().to_string(),
            name: w.name.clone(),
        })
        .collect()
}

/// A deterministic permutation from a seed (Fisher–Yates over an LCG).
fn shuffled<T: Clone>(items: &[T], seed: u64) -> Vec<T> {
    let mut out = items.to_vec();
    let mut s = seed | 1;
    for i in (1..out.len()).rev() {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (s >> 33) as usize % (i + 1);
        out.swap(i, j);
    }
    out
}

/// The same streets digitized the other way: every line reversed, a
/// one-way's spelling flipped so it still runs the same way.
fn redrawn(f: &Feature) -> Feature {
    Feature {
        lines: f.lines.iter().map(|l| l.iter().rev().copied().collect()).collect(),
        dir: match f.dir {
            Dir::Both => Dir::Both,
            Dir::Forward => Dir::Backward,
            Dir::Backward => Dir::Forward,
        },
        ..f.clone()
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// The document is the layer's streets: D-30's street test and
    /// class, the name, the travel direction, the geometry at 1e-6,
    /// in canonical order with no street twice. A layer with no
    /// street is an error.
    #[test]
    fn import_matches_the_oracle(features in layer(), junk in any::<bool>()) {
        let expected = expected_ways(&features);
        let got = import(&layer_bytes(&features, junk));
        if expected.is_empty() {
            prop_assert!(got.is_err(), "a layer with no street is an error, got {got:?}");
        } else {
            let doc = got.expect("a layer with a street imports");
            prop_assert_eq!(actual_ways(&doc), expected);
        }
    }

    /// The layer's feature order is not the streets.
    #[test]
    fn feature_order_does_not_change_the_document(features in layer(), seed in any::<u64>()) {
        let a = import(&layer_bytes(&features, false));
        let b = import(&layer_bytes(&shuffled(&features, seed), false));
        prop_assert_eq!(a.ok(), b.ok());
    }

    /// The direction a street was digitized in is not the streets; a
    /// one-way's travel direction is.
    #[test]
    fn digitized_direction_does_not_change_the_document(features in layer()) {
        let a = import(&layer_bytes(&features, false));
        let redrawn: Vec<Feature> = features.iter().map(redrawn).collect();
        let b = import(&layer_bytes(&redrawn, false));
        prop_assert_eq!(a.ok(), b.ok());
    }

    /// Properties outside the model are not the streets, and a street
    /// drawn twice is one street.
    #[test]
    fn foreign_properties_and_duplicates_do_not_change_the_document(features in layer()) {
        let a = import(&layer_bytes(&features, false));
        let b = import(&layer_bytes(&features, true));
        let doubled: Vec<Feature> = features.iter().chain(features.iter()).cloned().collect();
        let c = import(&layer_bytes(&doubled, true));
        prop_assert_eq!(a.as_ref().ok(), b.as_ref().ok());
        prop_assert_eq!(a.ok(), c.ok());
    }

    /// The import's document is valid, and survives its own JSON.
    #[test]
    fn the_import_is_a_valid_document(features in layer()) {
        if let Ok(doc) = import(&layer_bytes(&features, true)) {
            prop_assert!(doc.validate().is_ok(), "{:?}", doc.validate());
            let text = serde_json::to_string(&doc).unwrap();
            let back: Streets = serde_json::from_str(&text).unwrap();
            prop_assert_eq!(back, doc);
        }
    }

    /// Totality: arbitrary bytes are Ok or Err, never a panic.
    #[test]
    fn import_survives_arbitrary_bytes(bytes in vec(any::<u8>(), 0..=128)) {
        let _ = import(&bytes);
    }
}

// ---------------------------------------------------------------------
// Validity: a stored document the host hands back is checked, so a
// registered document is canonical — the hash of the streets, not of
// one spelling of them.
// ---------------------------------------------------------------------

fn doc(ways: Value) -> Streets {
    serde_json::from_value(json!({ "ways": ways })).unwrap()
}

#[test]
fn validate_refuses_what_the_import_never_writes() {
    let ok = doc(json!([
        { "points": [[45600000, -122600000], [45601000, -122600000]], "class": "local" },
        { "points": [[45601000, -122600000], [45601000, -122599000]], "oneway": true, "class": "local" },
    ]));
    assert!(ok.validate().is_ok(), "{:?}", ok.validate());

    let refused = [
        ("a lone point", json!([{ "points": [[45600000, -122600000]], "class": "local" }])),
        ("a repeated vertex", json!([{ "points": [[45600000, -122600000], [45600000, -122600000], [45601000, -122600000]], "class": "local" }])),
        ("a two-way drawn backward", json!([{ "points": [[45601000, -122600000], [45600000, -122600000]], "class": "local" }])),
        ("out of order", json!([
            { "points": [[45601000, -122600000], [45601000, -122599000]], "class": "local" },
            { "points": [[45600000, -122600000], [45601000, -122600000]], "class": "local" },
        ])),
        ("a street twice", json!([
            { "points": [[45600000, -122600000], [45601000, -122600000]], "class": "local" },
            { "points": [[45600000, -122600000], [45601000, -122600000]], "class": "local" },
        ])),
        ("off the globe", json!([{ "points": [[95000000, -122600000], [45601000, -122600000]], "class": "local" }])),
    ];
    for (what, ways) in refused {
        assert!(doc(ways).validate().is_err(), "{what} must be refused");
    }
}

/// C-TRAN's layer, in miniature: the railroad and the river are not
/// streets; SE Powell Blvd with a null CFCC is.
#[test]
fn railroads_and_rivers_are_not_streets() {
    let line = |props: Value, a: f64| {
        json!({ "type": "Feature", "properties": props,
                "geometry": { "type": "LineString", "coordinates": [[-122.6, a], [-122.6, a + 0.001]] } })
    };
    let fc = json!({ "type": "FeatureCollection", "features": [
        line(json!({ "CFCC": "B10", "FULLNAME": "BURLINGTON NORTHERN SANTA FE" }), 45.60),
        line(json!({ "CFCC": "H30", "FULLNAME": "WASHOUGAL RIVER" }), 45.61),
        line(json!({ "CFCC": null, "FULLNAME": "SE POWELL BLVD" }), 45.62),
        line(json!({ "CFCC": "A40", "FULLNAME": "NE 21ST AVE", "NAME": "21ST" }), 45.63),
    ]});
    let doc = import(&serde_json::to_vec(&fc).unwrap()).expect("import");
    let names: Vec<_> = doc.ways.iter().map(|w| w.name.as_deref().unwrap_or("")).collect();
    assert_eq!(names, ["SE POWELL BLVD", "NE 21ST AVE"]);
    let classes: Vec<_> = doc.ways.iter().map(|w| serde_json::to_value(w.class).unwrap()).collect();
    assert_eq!(classes, [json!("unknown"), json!("local")]);
}
