use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{RouteId, TripId};

pub type OperatorId = String;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Operator {
    pub name: String,
    pub url: String,
    pub timezone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fare_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub email: Option<String>,
    /// `None` is unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub contactless: Option<bool>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Contactless {
    Unknown,
    Accepted,
    NotAccepted,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Publisher {
    pub name: String,
    pub url: String,
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default_language: Option<String>,
    /// "YYYYMMDD".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub contact_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub contact_url: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// `None` on the attribution credits the whole dataset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum AttributionScope {
    Operator(OperatorId),
    Route(RouteId),
    Trip(TripId),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Attribution {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope: Option<AttributionScope>,
    pub organization: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub producer: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub operator: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub authority: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub phone: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// `table` and `field` use the names of the published dataset ("stops",
/// "stop_name"). A text is found by `record` (and `sub_record` where a record
/// needs two keys) or by its value in the original language.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Translation {
    pub table: String,
    pub field: String,
    pub language: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub record: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sub_record: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub value: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl crate::Network {
    /// GTFS requires every agency in a dataset to share `agency_timezone`,
    /// so operators that disagree, a network with none, or an empty zone
    /// answer `None`. Hosts read their clock in this zone, never in their
    /// own.
    pub fn timezone(&self) -> Option<&str> {
        let mut zones = self.operators.values().map(|o| o.timezone.as_str());
        let first = zones.next().filter(|z| !z.is_empty())?;
        zones.all(|z| z == first).then_some(first)
    }
}
