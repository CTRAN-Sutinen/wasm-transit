//! Who runs and publishes the network: the operators, the publisher
//! of the dataset, the organizations credited for it, and the text
//! of the network in other languages.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{RouteId, TripId};

pub type OperatorId = String;

/// An agency that runs service.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Operator {
    pub name: String,
    pub url: String,
    /// The timezone every time of the operator's service is in.
    pub timezone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub phone: Option<String>,
    /// Where riders buy fares online.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fare_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub email: Option<String>,
    /// Whether contactless bank cards are accepted; `None` = unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub contactless: Option<bool>,
}

/// Whether contactless bank cards are accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Contactless {
    Unknown,
    Accepted,
    NotAccepted,
}

/// Who publishes the dataset, and what it covers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Publisher {
    pub name: String,
    pub url: String,
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default_language: Option<String>,
    /// The first and last dates the dataset covers, "YYYYMMDD".
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
}

/// What an attribution credits: the whole dataset (`None` on the
/// attribution), or one operator, route, or trip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum AttributionScope {
    Operator(OperatorId),
    Route(RouteId),
    Trip(TripId),
}

/// An organization credited for the data.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
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
}

/// One text of the network in another language. `table` and `field`
/// name the text the way the published dataset does ("stops",
/// "stop_name"); the text is found by record (`record`, and
/// `sub_record` where a record needs two keys) or by its value in the
/// original language.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
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
}
