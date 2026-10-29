//! The streets document: the street network as the model holds it.
//!
//! A street layer (an agency GIS export, TIGER, OSM) is a
//! serialization; this is the model it is read into
//! ([`crate::geojson::import`]). The document is canonical, so its
//! content hash is the identity of the STREETS, not of one file
//! spelling them: the same streets in another feature order, drawn in
//! the other direction, or carrying other properties are the same
//! document.
//!
//! Canonical form ([`Streets::validate`] holds a document to it):
//!
//! * a way is two or more points with no point repeated in a row;
//! * points are `[lat, lon]` in integer microdegrees (1e-6°, about
//!   0.1 m) — the resolution at which ways weld into a graph;
//! * a one-way runs from its first point to its last; a two-way is
//!   drawn in whichever direction puts the smaller point sequence
//!   first;
//! * ways are sorted by points, then one-way, then class (by its wire
//!   name), then name (an absent name first), and no way appears
//!   twice.
//!
//! A pattern drawn on these streets commits GEOMETRY to the network
//! document, never a way or a graph position: a network never depends
//! on a streets hash.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::StreetError;

/// The street network: its ways, in canonical order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Streets {
    pub ways: Vec<Way>,
}

/// One street line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Way {
    /// `[lat, lon]` in microdegrees.
    pub points: Vec<[i32; 2]>,
    /// Travel only from the first point to the last.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub oneway: bool,
    pub class: StreetClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
}

/// What kind of street a way is (D-30). What each mode may use is a
/// mode profile's question (D-26), never the import's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum StreetClass {
    Motorway,
    Primary,
    Secondary,
    Local,
    Ramp,
    Service,
    Track,
    Path,
    Steps,
    /// A street whose layer gives no class the import knows.
    #[default]
    Unknown,
}

impl StreetClass {
    /// The wire name — the canonical order sorts by it.
    pub fn as_str(self) -> &'static str {
        match self {
            StreetClass::Motorway => "motorway",
            StreetClass::Primary => "primary",
            StreetClass::Secondary => "secondary",
            StreetClass::Local => "local",
            StreetClass::Ramp => "ramp",
            StreetClass::Service => "service",
            StreetClass::Track => "track",
            StreetClass::Path => "path",
            StreetClass::Steps => "steps",
            StreetClass::Unknown => "unknown",
        }
    }
}

const LAT_MAX: i32 = 90_000_000;
const LON_MAX: i32 = 180_000_000;

impl Way {
    /// The canonical order's key.
    fn key(&self) -> (&[[i32; 2]], bool, &'static str, Option<&str>) {
        (&self.points, self.oneway, self.class.as_str(), self.name.as_deref())
    }

    /// A two-way way drawn in its canonical direction.
    pub(crate) fn orient(&mut self) {
        if !self.oneway && self.points.iter().rev().lt(self.points.iter()) {
            self.points.reverse();
        }
    }
}

impl Streets {
    /// Ways in canonical form and order, no way twice.
    pub(crate) fn canonical(mut ways: Vec<Way>) -> Streets {
        for way in &mut ways {
            way.orient();
        }
        ways.sort_by(|a, b| a.key().cmp(&b.key()));
        ways.dedup();
        Streets { ways }
    }

    /// Hold a document to canonical form — the form the import writes,
    /// so a document registered from the host hashes as the import's.
    pub fn validate(&self) -> Result<(), StreetError> {
        for (i, way) in self.ways.iter().enumerate() {
            let bad = |what: &str| Err(StreetError(format!("way {i}: {what}")));
            if way.points.len() < 2 {
                return bad("fewer than two points");
            }
            if way.points.windows(2).any(|p| p[0] == p[1]) {
                return bad("a point repeated in a row");
            }
            if way.points.iter().any(|[lat, lon]| lat.abs() > LAT_MAX || lon.abs() > LON_MAX) {
                return bad("a point off the globe");
            }
            if !way.oneway && way.points.iter().rev().lt(way.points.iter()) {
                return bad("a two-way drawn against its canonical direction");
            }
            if i > 0 && self.ways[i - 1].key() >= way.key() {
                return bad("out of canonical order, or a way twice");
            }
        }
        Ok(())
    }
}
