//! The streets document is canonical ([`Streets::validate`] checks it), so
//! its content hash identifies the streets and not one file's spelling of
//! them. Points are `[lat, lon]` in integer microdegrees (1e-6 degrees, about
//! 0.1 m), the resolution at which ways weld into a graph.
//!
//! A pattern drawn on these streets commits geometry to the network
//! document, never a way or a graph position, so a network never depends on
//! a streets hash.

use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

use crate::StreetError;

transit_version::kind_tag!(StreetsKind = "streets");

impl transit_version::Versioned for Streets {
    const KIND: &'static str = "streets";
    const READS: u32 = 1;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Streets {
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"streets\""))]
    pub kind: StreetsKind,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<1>,
    pub ways: Vec<Way>,
}

impl Streets {
    /// Counts (ways, segments, distinct points) without building the graph.
    pub fn counts(&self) -> (u32, u32, u32) {
        let segments = self
            .ways
            .iter()
            .map(|w| w.points.len().saturating_sub(1) as u32)
            .sum();
        let nodes = self
            .ways
            .iter()
            .flat_map(|w| &w.points)
            .collect::<std::collections::HashSet<_>>()
            .len() as u32;
        (self.ways.len() as u32, segments, nodes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Way {
    pub points: Vec<[i32; 2]>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<bool>"))]
    pub oneway: bool,
    pub class: StreetClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub name: Option<String>,
}

/// A mode profile decides what each mode may use. The import does
/// not (D-30).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
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
    #[default]
    Unknown,
}

impl StreetClass {
    /// The wire name, which the canonical order sorts by.
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
    fn key(&self) -> (&[[i32; 2]], bool, &'static str, Option<&str>) {
        (
            &self.points,
            self.oneway,
            self.class.as_str(),
            self.name.as_deref(),
        )
    }

    pub(crate) fn orient(&mut self) {
        if !self.oneway && self.points.iter().rev().lt(self.points.iter()) {
            self.points.reverse();
        }
    }
}

impl Streets {
    pub(crate) fn canonical(mut ways: Vec<Way>) -> Streets {
        for way in &mut ways {
            way.orient();
        }
        ways.sort_by(|a, b| a.key().cmp(&b.key()));
        ways.dedup();
        Streets {
            kind: Default::default(),
            schema_version: Default::default(),
            ways,
        }
    }

    /// Checks canonical form, so a registered document hashes as the import
    /// hashes it.
    pub fn validate(&self) -> Result<(), StreetError> {
        for (i, way) in self.ways.iter().enumerate() {
            let bad = |what: &str| Err(StreetError(format!("way {i}: {what}")));
            if way.points.len() < 2 {
                return bad("fewer than two points");
            }
            if way.points.windows(2).any(|p| p[0] == p[1]) {
                return bad("a point repeated in a row");
            }
            if way
                .points
                .iter()
                .any(|[lat, lon]| lat.abs() > LAT_MAX || lon.abs() > LON_MAX)
            {
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
