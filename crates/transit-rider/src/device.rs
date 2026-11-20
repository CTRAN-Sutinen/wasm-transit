//! A rider's device records: what ride-now keeps for its rider on one
//! device, as tagged documents in the browser's document store
//! (`web/src/documents.svelte.ts`; docs/FORMATS.md, "Versions"). They
//! never leave the device. The map's last frame and location fix are
//! presentation preferences, and stay out of the store.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

transit_version::kind_tag!(
    /// The recent places' kind tag: always `"recent_places"`.
    RecentPlacesKind = "recent_places"
);

transit_version::kind_tag!(
    /// The live feeds' kind tag: always `"live_feeds"`.
    LiveFeedsKind = "live_feeds"
);

/// The places a rider looked at last on this device, newest first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "cap_board/")]
pub struct RecentPlaces {
    /// The kind tag and version (D-115); a copy without them is this
    /// kind, version 1.
    #[serde(default)]
    #[ts(type = "\"recent_places\"")]
    pub kind: RecentPlacesKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<1>,
    pub places: Vec<RecentPlace>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// One recent place: a stop by its id and name, and its point when the
/// rider's device has it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "cap_board/")]
pub struct RecentPlace {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lat: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lon: Option<f64>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// The rider's own GTFS-rt feed URLs, in place of the deployment's
/// (`live-feeds.json`). An empty URL is a feed the rider does not read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "cap_board/")]
pub struct LiveFeeds {
    /// The kind tag and version (D-115); a copy without them is this
    /// kind, version 1.
    #[serde(default)]
    #[ts(type = "\"live_feeds\"")]
    pub kind: LiveFeedsKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<1>,
    #[serde(default)]
    pub trips: String,
    #[serde(default)]
    pub vehicles: String,
    #[serde(default)]
    pub alerts: String,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
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
