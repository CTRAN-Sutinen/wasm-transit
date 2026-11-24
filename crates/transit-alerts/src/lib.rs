//! The service alerts document and the alert templates document
//! (D-206..D-210).
//!
//! A service alert is what the agency tells riders: its cause, effect,
//! and severity, its text in each language, when to show it (the
//! communication periods) and when service is affected (the impact
//! periods), and what it is about (the informed entities), all in the
//! network's ids. Dispatch and the communications staff write alerts,
//! and every team reads the one list (D-207). GTFS-rt is the alerts'
//! serialization: transit-rt encodes the published ones as an Alerts
//! feed.
//!
//! Each document is a set of records by id, changed by replacing a
//! record whole. Nothing is deleted. An alert that will not go out is
//! `retired`, and an alert that ends gets its end ([`ServiceAlerts::end`]).
//! The log keeps every state of every record, and who wrote it.
//!
//! [`ServiceAlerts::checked`] is what a write must pass. It refuses only
//! what makes a record wrong (D-152). [`ServiceAlerts::flags`] is what a
//! person is told about a record that passes: an id the network does
//! not have, or a missing translation.
//!
//! [`fill`] writes a template's standard language with its blanks
//! filled in. What is in force that no alert explains is
//! transit-disruptions' gap list (D-218).

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use ts_rs::TS;


pub mod version;
pub mod load;

pub use version::{ALERTS_VERSION, TEMPLATES_VERSION};

/// Why an alert was raised (GTFS-rt's cause, by name).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AlertCause {
    #[default]
    Unknown,
    Other,
    TechnicalProblem,
    Strike,
    Demonstration,
    Accident,
    Holiday,
    Weather,
    Maintenance,
    Construction,
    PoliceActivity,
    MedicalEmergency,
    SpecialEvent,
}

/// What an alert does to service (GTFS-rt's effect, by name).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AlertEffect {
    NoService,
    ReducedService,
    SignificantDelays,
    Detour,
    AdditionalService,
    ModifiedService,
    Other,
    #[default]
    Unknown,
    StopMoved,
    NoEffect,
    AccessibilityIssue,
}

/// How serious the agency says an alert is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    #[default]
    Unknown,
    Info,
    Warning,
    Severe,
}

/// A span of time in Unix seconds; an open end is unbounded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlertPeriod {
    #[ts(type = "number | null")]
    pub start: Option<u64>,
    #[ts(type = "number | null")]
    pub end: Option<u64>,
}

transit_version::kind_tag!(
    /// The service alerts document's kind tag: always `"service_alerts"`.
    ServiceAlertsKind = "service_alerts"
);

transit_version::kind_tag!(
    /// The alert templates document's kind tag: always `"alert_templates"`.
    AlertTemplatesKind = "alert_templates"
);

impl transit_version::Versioned for ServiceAlerts {
    const KIND: &'static str = "service_alerts";
    const READS: u32 = ALERTS_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

impl transit_version::Versioned for AlertTemplates {
    const KIND: &'static str = "alert_templates";
    const READS: u32 = TEMPLATES_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

// ---------------------------------------------------------------------
// The service alerts document
// ---------------------------------------------------------------------

/// Every service alert, by its id. The id is the alert's GTFS-rt entity
/// id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct ServiceAlerts {
    /// The kind tag and version (D-115). A copy without the version is
    /// version 1, which cap-migrate brings forward ([`version`]).
    #[serde(default)]
    #[ts(type = "\"service_alerts\"")]
    pub kind: ServiceAlertsKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<2>,
    pub alerts: BTreeMap<String, ServiceAlert>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// An alert's state. There is no review step (D-207): a person with
/// the write role publishes at once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AlertStatus {
    /// Saved, not yet told to riders.
    #[default]
    Draft,
    /// Told to riders while a communication period holds.
    Published,
    /// Withdrawn. It is kept, and is never told to riders.
    Retired,
}

/// One service alert.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct ServiceAlert {
    pub status: AlertStatus,
    #[serde(default)]
    pub cause: AlertCause,
    #[serde(default)]
    pub effect: AlertEffect,
    #[serde(default)]
    pub severity: AlertSeverity,
    /// The header in each language. At least one text is not blank.
    pub header: Vec<AlertText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<AlertText>>")]
    pub description: Vec<AlertText>,
    /// A link to more about it, in each language.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<AlertText>>")]
    pub url: Vec<AlertText>,
    /// When to show it, in Unix seconds (the host converts the agency's
    /// local times; D-205). None means for as long as it is published.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<AlertPeriod>>")]
    pub communication: Vec<AlertPeriod>,
    /// When service is affected, in Unix seconds. Each impact period is
    /// inside a communication period (the GTFS-rt spec's rule).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<AlertPeriod>>")]
    pub impact: Vec<AlertPeriod>,
    /// What it is about: one entity for each affected entity, each
    /// applying on its own. The fields of one entity are joined by AND.
    /// At least one.
    pub informed: Vec<AlertEntity>,
    /// The disruptions it tells riders about: a diversion or trip
    /// cancel, or a suspended stop. Nothing writes an alert from these
    /// automatically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<AlertSource>>")]
    pub explains: Vec<AlertSource>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// A text in one language. No language means the agency's default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlertText {
    /// A BCP 47 language tag ("en", "es").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lang: Option<String>,
    pub text: String,
}

/// One informed entity, in the network's ids. Every present field must
/// match (route 12 AT stop 6277). With every field absent, it is the
/// whole agency.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlertEntity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[schemars(extend("x-references" = "network.route"))]
    pub route: Option<String>,
    /// Every route published under this GTFS route type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub route_type: Option<u16>,
    /// One of the route's model directions. It needs `route` (D-204).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub direction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[schemars(extend("x-references" = "network.stop"))]
    pub stop: Option<String>,
    /// A trip, by the model's trip id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[schemars(extend("x-references" = "network.trip"))]
    pub trip: Option<String>,
    /// The trip instance's service date, "YYYY-MM-DD". None means the
    /// trip on every date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub date: Option<String>,
    /// The trip instance's start, in seconds after the service-day
    /// anchor. It needs `trip`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_secs: Option<u32>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// A disruption that an alert tells riders about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlertSource {
    /// A record of the edit document (a diversion or a trip cancel), by
    /// its record id.
    Edit {
        #[schemars(extend("x-references" = "edit_record"))]
        id: String,
    },
    /// A stop's suspension in the stops document, by the stop's lasting
    /// id.
    Stop {
        #[schemars(extend("x-references" = "stop"))]
        stop: String,
    },
    /// A trip instance that the live feed cancels (D-216): the model's
    /// trip id, its service date ("YYYY-MM-DD"), and its start in seconds
    /// after the service-day anchor. Version 2.
    #[schemars(extend("x-references" = "trip_instance"))]
    Trip { trip: String, date: String, start_secs: u32 },
}

fn in_periods(periods: &[AlertPeriod], t: u64) -> bool {
    periods.is_empty() || periods.iter().any(|p| p.start.is_none_or(|s| s <= t) && p.end.is_none_or(|e| t <= e))
}

impl ServiceAlert {
    /// Is it told to riders at `t` (Unix seconds)? Only a published
    /// alert is, while a communication period holds; with none, always.
    pub fn in_force(&self, t: u64) -> bool {
        self.status == AlertStatus::Published && in_periods(&self.communication, t)
    }

    /// Have its communication periods all ended by `t`? An alert with
    /// none never ends.
    pub fn ended_by(&self, t: u64) -> bool {
        !self.communication.is_empty() && self.communication.iter().all(|p| p.end.is_some_and(|e| e < t))
    }

    /// Does any communication period start after `t`, and none hold at
    /// `t`?
    fn upcoming_at(&self, t: u64) -> bool {
        !in_periods(&self.communication, t) && self.communication.iter().any(|p| p.start.is_some_and(|s| s > t))
    }
}

impl ServiceAlerts {
    /// One alert placed by id, new or replacing the one held. Every
    /// other alert as it was. Unchecked: a write passes
    /// [`ServiceAlerts::checked`].
    pub fn put(&self, id: &str, alert: ServiceAlert) -> ServiceAlerts {
        let mut next = self.clone();
        next.alerts.insert(id.to_string(), alert);
        next
    }

    /// Alert `id` ended at `at` (Unix seconds): a period that holds at
    /// `at` or later ends at `at`, and a period that starts after `at`
    /// goes. Nothing up to `at` changes. An alert with no period gets
    /// one that ends at `at`. An alert with no period left at or before
    /// `at` was never told to riders, so it is withdrawn (`retired`).
    /// None for an id the document does not hold.
    pub fn end(&self, id: &str, at: u64) -> Option<ServiceAlerts> {
        let mut next = self.clone();
        let a = next.alerts.get_mut(id)?;
        let clip = |periods: &[AlertPeriod]| -> Vec<AlertPeriod> {
            periods
                .iter()
                .filter(|p| p.start.is_none_or(|s| s <= at))
                .map(|p| AlertPeriod { start: p.start, end: Some(p.end.map_or(at, |e| e.min(at))) })
                .collect()
        };
        if a.communication.is_empty() {
            a.communication = vec![AlertPeriod { start: None, end: Some(at) }];
            a.impact = clip(&a.impact);
        } else {
            let communication = clip(&a.communication);
            if communication.is_empty() {
                a.status = AlertStatus::Retired;
            } else {
                a.communication = communication;
                a.impact = clip(&a.impact);
            }
        }
        Some(next)
    }

    /// The agency's list at `now` (Unix seconds). Every alert is in one
    /// group, in id order.
    pub fn list(&self, now: u64) -> AlertList {
        let mut list = AlertList::default();
        for (id, a) in &self.alerts {
            let row = |open_secs| AlertRow { id: id.clone(), alert: a.clone(), open_secs };
            match a.status {
                AlertStatus::Draft => list.drafts.push(row(None)),
                AlertStatus::Retired => list.ended.push(row(None)),
                AlertStatus::Published if a.in_force(now) => {
                    let open = a
                        .communication
                        .iter()
                        .filter(|p| p.end.is_none())
                        .filter_map(|p| p.start.filter(|s| *s <= now))
                        .min()
                        .map(|s| now - s);
                    list.in_force.push(row(open));
                }
                AlertStatus::Published if a.upcoming_at(now) => list.upcoming.push(row(None)),
                AlertStatus::Published => list.ended.push(row(None)),
            }
        }
        list
    }

    /// The document, if a write may store it, or every reason it may
    /// not.
    pub fn checked(self) -> Result<Self, Vec<AlertRefusal>> {
        let refusals: Vec<AlertRefusal> = self
            .alerts
            .iter()
            .flat_map(|(id, a)| check(a).into_iter().map(move |problem| AlertRefusal { alert: id.clone(), problem }))
            .chain(self.alerts.keys().filter(|k| k.trim().is_empty()).map(|_| AlertRefusal { alert: String::new(), problem: AlertProblem::NoId }))
            .collect();
        if refusals.is_empty() {
            Ok(self)
        } else {
            Err(refusals)
        }
    }

    /// What a person is told about the alerts that are not retired: each
    /// id the network does not have, and each text with no translation
    /// for a language of `languages` (the deployment's). Never a reason
    /// to refuse a write.
    pub fn flags(&self, network: &transit_network::Network, languages: &[String]) -> Vec<AlertFlag> {
        let mut out = Vec::new();
        for (id, a) in self.alerts.iter().filter(|(_, a)| a.status != AlertStatus::Retired) {
            let mut push = |notice| out.push(AlertFlag { alert: id.clone(), notice });
            for e in &a.informed {
                if let Some(r) = &e.route {
                    match network.routes.get(r) {
                        None => push(AlertNotice::UnknownRoute { route: r.clone() }),
                        Some(route) => {
                            if let Some(d) = e.direction.as_ref().filter(|d| !route.directions.contains_key(*d)) {
                                push(AlertNotice::UnknownDirection { route: r.clone(), direction: d.clone() });
                            }
                        }
                    }
                }
                if let Some(s) = e.stop.as_ref().filter(|s| !network.stops.contains_key(*s)) {
                    push(AlertNotice::UnknownStop { stop: s.clone() });
                }
                if let Some(t) = e.trip.as_ref().filter(|t| network.trip(t).is_none()) {
                    push(AlertNotice::UnknownTrip { trip: t.clone() });
                }
            }
            for (field, texts) in [("header", &a.header), ("description", &a.description), ("url", &a.url)] {
                if field != "header" && texts.is_empty() {
                    continue;
                }
                for lang in languages {
                    if !texts.iter().any(|t| t.lang.as_deref() == Some(lang.as_str()) && !t.text.trim().is_empty()) {
                        push(AlertNotice::MissingTranslation { field: field.into(), lang: lang.clone() });
                    }
                }
            }
        }
        out
    }
}

/// The agency's list of alerts at a moment (D-207).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlertList {
    /// Published and told to riders now.
    pub in_force: Vec<AlertRow>,
    /// Published, and told to riders later.
    pub upcoming: Vec<AlertRow>,
    /// Published and ended, or withdrawn.
    pub ended: Vec<AlertRow>,
    pub drafts: Vec<AlertRow>,
}

/// One alert in the list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlertRow {
    pub id: String,
    pub alert: ServiceAlert,
    /// For an alert in force with no end: how long it has been told to
    /// riders, in seconds. A long-open alert is made visible by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub open_secs: Option<u64>,
}

/// Why a write is refused: the alert, and the problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlertRefusal {
    pub alert: String,
    pub problem: AlertProblem,
}

/// What is wrong with an alert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlertProblem {
    /// An alert with an empty id.
    NoId,
    /// No informed entity: the alert is about nothing.
    NoEntity,
    /// No header text that is not blank.
    NoHeader,
    /// A period that ends before it starts.
    EndBeforeStart,
    /// An impact period outside every communication period.
    ImpactOutsideCommunication,
    /// A direction with no route (GTFS-rt requires `route_id` with
    /// `direction_id`).
    DirectionWithoutRoute,
    /// A trip instance's date or start with no trip.
    InstanceWithoutTrip,
    /// A date that is not "YYYY-MM-DD".
    BadDate { date: String },
    /// Two texts of one field in the same language.
    RepeatedLanguage { field: String, lang: Option<String> },
}

impl fmt::Display for AlertRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match &self.problem {
            AlertProblem::NoId => "has no id".to_string(),
            AlertProblem::NoEntity => "is about nothing: it names no route, stop, trip, or agency".into(),
            AlertProblem::NoHeader => "has no header".into(),
            AlertProblem::EndBeforeStart => "has a period that ends before it starts".into(),
            AlertProblem::ImpactOutsideCommunication => "has an impact period outside every communication period".into(),
            AlertProblem::DirectionWithoutRoute => "names a direction with no route".into(),
            AlertProblem::InstanceWithoutTrip => "names a trip date or start with no trip".into(),
            AlertProblem::BadDate { date } => format!("has {date:?}, not a YYYY-MM-DD date"),
            AlertProblem::RepeatedLanguage { field, lang } => format!("has two {field} texts in {}", lang.as_deref().unwrap_or("the default language")),
        };
        write!(f, "alert {:?} {what}", self.alert)
    }
}

/// What a person is told about an alert that passes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlertFlag {
    pub alert: String,
    pub notice: AlertNotice,
}

/// A consequence of an alert, for a person to look at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlertNotice {
    UnknownRoute { route: String },
    UnknownDirection { route: String, direction: String },
    UnknownStop { stop: String },
    UnknownTrip { trip: String },
    /// A language the deployment names has no text in this field.
    MissingTranslation { field: String, lang: String },
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn bad_period(p: &AlertPeriod) -> bool {
    matches!((p.start, p.end), (Some(s), Some(e)) if e < s)
}

fn inside(inner: &AlertPeriod, outer: &AlertPeriod) -> bool {
    let starts = match (outer.start, inner.start) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(o), Some(i)) => o <= i,
    };
    let ends = match (outer.end, inner.end) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(o), Some(i)) => i <= o,
    };
    starts && ends
}

fn repeated_languages(field: &str, texts: &[AlertText], out: &mut Vec<AlertProblem>) {
    let mut seen = std::collections::BTreeSet::new();
    for t in texts {
        if !seen.insert(t.lang.clone()) {
            out.push(AlertProblem::RepeatedLanguage { field: field.into(), lang: t.lang.clone() });
        }
    }
}

fn check(a: &ServiceAlert) -> Vec<AlertProblem> {
    let mut out = Vec::new();
    if a.informed.is_empty() {
        out.push(AlertProblem::NoEntity);
    }
    if !a.header.iter().any(|t| !t.text.trim().is_empty()) {
        out.push(AlertProblem::NoHeader);
    }
    if a.communication.iter().chain(&a.impact).any(bad_period) {
        out.push(AlertProblem::EndBeforeStart);
    } else if !a.communication.is_empty() && a.impact.iter().any(|i| !a.communication.iter().any(|c| inside(i, c))) {
        out.push(AlertProblem::ImpactOutsideCommunication);
    }
    for e in &a.informed {
        if e.direction.is_some() && e.route.is_none() {
            out.push(AlertProblem::DirectionWithoutRoute);
        }
        if e.trip.is_none() && (e.date.is_some() || e.start_secs.is_some()) {
            out.push(AlertProblem::InstanceWithoutTrip);
        }
        if let Some(d) = e.date.as_ref().filter(|d| parse_date(d).is_none()) {
            out.push(AlertProblem::BadDate { date: d.clone() });
        }
    }
    for source in &a.explains {
        if let AlertSource::Trip { trip, date, .. } = source {
            if trip.trim().is_empty() {
                out.push(AlertProblem::InstanceWithoutTrip);
            }
            if parse_date(date).is_none() {
                out.push(AlertProblem::BadDate { date: date.clone() });
            }
        }
    }
    repeated_languages("header", &a.header, &mut out);
    repeated_languages("description", &a.description, &mut out);
    repeated_languages("url", &a.url, &mut out);
    out
}

// ---------------------------------------------------------------------
// The alert templates document
// ---------------------------------------------------------------------

/// The blanks a template's text may hold, each written in braces
/// ("{stop}").
pub const BLANKS: [&str; 4] = ["stop", "route", "start date", "end date"];

/// Every alert template, by its id: the agency's standard language
/// (D-207, D-210).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlertTemplates {
    /// The kind tag and version (D-115); a copy without them is this
    /// kind, version 1.
    #[serde(default)]
    #[ts(type = "\"alert_templates\"")]
    pub kind: AlertTemplatesKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<1>,
    pub templates: BTreeMap<String, AlertTemplate>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// One template: its name in the menu, the cause, effect, and severity
/// it sets, and its texts with blanks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlertTemplate {
    pub name: String,
    #[serde(default)]
    pub cause: AlertCause,
    #[serde(default)]
    pub effect: AlertEffect,
    #[serde(default)]
    pub severity: AlertSeverity,
    pub header: Vec<AlertText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<AlertText>>")]
    pub description: Vec<AlertText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<AlertText>>")]
    pub url: Vec<AlertText>,
    /// No longer offered in the menu. It is kept.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub retired: bool,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// Why a template write is refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct TemplateRefusal {
    pub template: String,
    pub problem: TemplateProblem,
}

/// What is wrong with a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemplateProblem {
    NoId,
    NoName,
    NoHeader,
    /// A blank that is not one of [`BLANKS`]: it could never be filled.
    UnknownBlank { blank: String },
    RepeatedLanguage { field: String, lang: Option<String> },
}

impl fmt::Display for TemplateRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match &self.problem {
            TemplateProblem::NoId => "has no id".to_string(),
            TemplateProblem::NoName => "has no name".into(),
            TemplateProblem::NoHeader => "has no header".into(),
            TemplateProblem::UnknownBlank { blank } => format!("has the blank {{{blank}}}, which is not one of {}", BLANKS.map(|b| format!("{{{b}}}")).join(", ")),
            TemplateProblem::RepeatedLanguage { field, lang } => format!("has two {field} texts in {}", lang.as_deref().unwrap_or("the default language")),
        };
        write!(f, "template {:?} {what}", self.template)
    }
}

/// The blanks written in a text, in order.
fn blanks_in(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        out.push(&after[..close]);
        rest = &after[close + 1..];
    }
    out
}

/// One template as a menu offers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct TemplateRow {
    pub id: String,
    pub name: String,
    pub cause: AlertCause,
    pub effect: AlertEffect,
    pub severity: AlertSeverity,
}

impl AlertTemplates {
    /// The templates offered, by name, then id. A retired template is
    /// kept and not offered.
    pub fn offered(&self) -> Vec<TemplateRow> {
        let mut rows: Vec<TemplateRow> = self
            .templates
            .iter()
            .filter(|(_, t)| !t.retired)
            .map(|(id, t)| TemplateRow { id: id.clone(), name: t.name.clone(), cause: t.cause, effect: t.effect, severity: t.severity })
            .collect();
        rows.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
        rows
    }

    /// One template placed by id, new or replacing the one held.
    /// Unchecked: a write passes [`AlertTemplates::checked`].
    pub fn put(&self, id: &str, template: AlertTemplate) -> AlertTemplates {
        let mut next = self.clone();
        next.templates.insert(id.to_string(), template);
        next
    }

    /// The document, if a write may store it, or every reason it may
    /// not.
    pub fn checked(self) -> Result<Self, Vec<TemplateRefusal>> {
        let mut refusals = Vec::new();
        for (id, t) in &self.templates {
            let mut push = |problem| refusals.push(TemplateRefusal { template: id.clone(), problem });
            if id.trim().is_empty() {
                push(TemplateProblem::NoId);
            }
            if t.name.trim().is_empty() {
                push(TemplateProblem::NoName);
            }
            if !t.header.iter().any(|x| !x.text.trim().is_empty()) {
                push(TemplateProblem::NoHeader);
            }
            let texts = t.header.iter().chain(&t.description).chain(&t.url);
            let mut unknown: Vec<&str> = texts.flat_map(|x| blanks_in(&x.text)).filter(|b| !BLANKS.contains(b)).collect();
            unknown.dedup();
            for blank in unknown {
                push(TemplateProblem::UnknownBlank { blank: blank.to_string() });
            }
            let mut problems = Vec::new();
            for (field, texts) in [("header", &t.header), ("description", &t.description), ("url", &t.url)] {
                let mut seen = std::collections::BTreeSet::new();
                for x in texts {
                    if !seen.insert(x.lang.clone()) {
                        problems.push(TemplateProblem::RepeatedLanguage { field: field.into(), lang: x.lang.clone() });
                    }
                }
            }
            for p in problems {
                push(p);
            }
        }
        if refusals.is_empty() {
            Ok(self)
        } else {
            Err(refusals)
        }
    }
}

/// What a template's blanks are filled with: the stops and routes by
/// the network's ids, and the dates as "YYYY-MM-DD" (the host's, in the
/// agency's zone).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct FillInput {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub stops: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub routes: Vec<String>,
    /// The alert's informed entities: each route and stop in them is
    /// named once, after the `routes` and `stops` above.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<AlertEntity>>")]
    pub informed: Vec<AlertEntity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end_date: Option<String>,
}

impl FillInput {
    /// The stops to name: those the input lists, then the informed
    /// entities' stops, each once.
    fn stop_ids(&self) -> Vec<String> {
        unique(self.stops.iter().cloned().chain(self.informed.iter().filter_map(|e| e.stop.clone())))
    }

    /// The routes to name: those the input lists, then the informed
    /// entities' routes, each once.
    fn route_ids(&self) -> Vec<String> {
        unique(self.routes.iter().cloned().chain(self.informed.iter().filter_map(|e| e.route.clone())))
    }
}

fn unique(ids: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// A template written out: its cause, effect, and severity, its texts
/// with the blanks filled, and each blank it could not fill.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Filled {
    pub cause: AlertCause,
    pub effect: AlertEffect,
    pub severity: AlertSeverity,
    pub header: Vec<AlertText>,
    pub description: Vec<AlertText>,
    pub url: Vec<AlertText>,
    /// Each blank left in a text, for a person to fill by hand.
    pub unfilled: Vec<Unfilled>,
}

/// A blank the fill left in a text: the field, the text's language, and
/// the blank.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Unfilled {
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lang: Option<String>,
    pub blank: String,
}

/// Is this text in English (the only language whose dates and lists the
/// fill writes)? No language is the agency's default, taken as English.
fn english(lang: Option<&str>) -> bool {
    lang.is_none_or(|l| l == "en" || l.starts_with("en-"))
}

/// "A", "A and B", "A, B, and C".
fn english_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// "Monday, August 17".
fn english_date(d: NaiveDate) -> String {
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let weekday = match d.weekday() {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    };
    format!("{weekday}, {} {}", MONTHS[d.month0() as usize], d.day())
}

/// The value of one blank in a text of `lang`, or none when the fill
/// cannot write it: nothing given, an id the network does not have, a
/// date that is not "YYYY-MM-DD", or a date or a list of two or more in
/// a language other than English.
fn value(blank: &str, lang: Option<&str>, network: &transit_network::Network, input: &FillInput) -> Option<String> {
    let names = |ids: &[String], name: &dyn Fn(&str) -> Option<String>| -> Option<String> {
        if ids.is_empty() || (ids.len() > 1 && !english(lang)) {
            return None;
        }
        let names: Option<Vec<String>> = ids.iter().map(|id| name(id)).collect();
        Some(english_list(&names?))
    };
    let date = |d: &Option<String>| -> Option<String> {
        let d = parse_date(d.as_deref()?)?;
        english(lang).then(|| english_date(d))
    };
    match blank {
        "stop" => names(&input.stop_ids(), &|id| network.stops.get(id).map(|s| s.name.clone())),
        "route" => names(&input.route_ids(), &|id| network.routes.get(id).map(|r| r.label().to_string())),
        "start date" => date(&input.start_date),
        "end date" => date(&input.end_date),
        _ => None,
    }
}

/// The template written out for `input`. Each blank the fill can write
/// is replaced; each it cannot is left as written and named in
/// `unfilled`.
pub fn fill(template: &AlertTemplate, network: &transit_network::Network, input: &FillInput) -> Filled {
    let mut unfilled = Vec::new();
    let mut texts = |field: &str, texts: &[AlertText]| -> Vec<AlertText> {
        texts
            .iter()
            .map(|t| {
                let mut out = String::new();
                let mut rest = t.text.as_str();
                let mut left: Vec<String> = Vec::new();
                while let Some(open) = rest.find('{') {
                    let after = &rest[open + 1..];
                    let Some(close) = after.find('}') else { break };
                    let blank = &after[..close];
                    out.push_str(&rest[..open]);
                    match value(blank, t.lang.as_deref(), network, input) {
                        Some(v) => out.push_str(&v),
                        None => {
                            out.push_str(&rest[open..open + close + 2]);
                            if !left.iter().any(|b| b == blank) {
                                left.push(blank.to_string());
                            }
                        }
                    }
                    rest = &after[close + 1..];
                }
                out.push_str(rest);
                unfilled.extend(left.into_iter().map(|blank| Unfilled { field: field.into(), lang: t.lang.clone(), blank }));
                AlertText { lang: t.lang.clone(), text: out }
            })
            .collect()
    };
    let header = texts("header", &template.header);
    let description = texts("description", &template.description);
    let url = texts("url", &template.url);
    Filled { cause: template.cause, effect: template.effect, severity: template.severity, header, description, url, unfilled }
}
