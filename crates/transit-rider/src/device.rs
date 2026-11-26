//! These documents never leave the device. The map's last frame and location
//! fix are presentation preferences and stay out of the store
//! (docs/FORMATS.md, "Versions").

use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

transit_version::kind_tag!(
    RecentPlacesKind = "recent_places"
);

transit_version::kind_tag!(
    LiveFeedsKind = "live_feeds"
);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_board/"))]
pub struct RecentPlaces {
    /// A copy without the kind tag and version is this kind, version 1.
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"recent_places\""))]
    pub kind: RecentPlacesKind,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<1>,
    pub places: Vec<RecentPlace>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_board/"))]
pub struct RecentPlace {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub lat: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub lon: Option<f64>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// An empty URL is a feed that the rider does not read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_board/"))]
pub struct LiveFeeds {
    /// A copy without the kind tag and version is this kind, version 1.
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"live_feeds\""))]
    pub kind: LiveFeedsKind,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<1>,
    #[serde(default)]
    pub trips: String,
    #[serde(default)]
    pub vehicles: String,
    #[serde(default)]
    pub alerts: String,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl transit_version::Versioned for RecentPlaces {
    const KIND: &'static str = "recent_places";
    const READS: u32 = 1;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

impl transit_version::Versioned for LiveFeeds {
    const KIND: &'static str = "live_feeds";
    const READS: u32 = 1;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}
