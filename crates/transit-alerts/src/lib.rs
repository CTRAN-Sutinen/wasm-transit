//! The service alerts document and the alert templates document (D-206, D-207,
//! D-209).
//!
//! An alert is its own record in its own document, not a payload of the edit
//! document, because an alert changes no service. The board and router patch
//! the day from edits only.
//!
//! The record keeps every translation, and the live document carries one
//! (D-20). An alert can explain a trip instance that the live feed cancels,
//! because on C-TRAN the feed is where cancels come from (D-213). Nothing
//! writes an alert from a disruption. `explains` names the trip instance as a
//! third kind of source. A separate field would give one idea two fields, and
//! matching by the alert's entities would be a rule, not a recorded fact.
//! What no alert explains is transit-disruptions' gap list.
//!
//! [`fill`] fills a template's blanks, so names and dates read the same in
//! every alert. A blank that it cannot fill stays and is named.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use transit_date::Date;
#[cfg(feature = "bindings")]
use ts_rs::TS;

pub mod load;
pub mod version;

pub use version::{ALERTS_VERSION, TEMPLATES_VERSION};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    #[default]
    Unknown,
    Info,
    Warning,
    Severe,
}

/// Unix seconds. An open end is unbounded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct AlertPeriod {
    #[cfg_attr(feature = "bindings", ts(type = "number | null"))]
    pub start: Option<u64>,
    #[cfg_attr(feature = "bindings", ts(type = "number | null"))]
    pub end: Option<u64>,
}

transit_version::kind_tag!(ServiceAlertsKind = "service_alerts");

transit_version::kind_tag!(AlertTemplatesKind = "alert_templates");

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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ServiceAlerts {
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"service_alerts\""))]
    pub kind: ServiceAlertsKind,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<2>,
    pub alerts: BTreeMap<String, ServiceAlert>,
    /// Fields this build does not know, kept as written.
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// There is no review step (D-207).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum AlertStatus {
    #[default]
    Draft,
    Published,
    Retired,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ServiceAlert {
    pub status: AlertStatus,
    #[serde(default)]
    pub cause: AlertCause,
    #[serde(default)]
    pub effect: AlertEffect,
    #[serde(default)]
    pub severity: AlertSeverity,
    pub header: Vec<AlertText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<AlertText>>"))]
    pub description: Vec<AlertText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<AlertText>>"))]
    pub url: Vec<AlertText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<AlertPeriod>>"))]
    pub communication: Vec<AlertPeriod>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<AlertPeriod>>"))]
    pub impact: Vec<AlertPeriod>,
    pub informed: Vec<AlertEntity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<AlertSource>>"))]
    pub explains: Vec<AlertSource>,
    /// Fields this build does not know, kept as written.
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// A text in one language. No language means the agency's default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct AlertText {
    /// A BCP 47 language tag ("en", "es").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub lang: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct AlertEntity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "network.route")))]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub route_type: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub direction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "network.stop")))]
    pub stop: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "network.trip")))]
    pub trip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub date: Option<String>,
    /// Seconds after the service-day anchor. Needs `trip`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub start_secs: Option<u32>,
    /// Fields this build does not know, kept as written.
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlertSource {
    Edit {
        #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "edit_record")))]
        id: String,
    },
    Stop {
        #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "stop")))]
        stop: String,
    },
    /// A trip instance that the live feed cancels. Version 2.
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "trip_instance")))]
    Trip {
        trip: String,
        date: String,
        start_secs: u32,
    },
}

fn in_periods(periods: &[AlertPeriod], t: u64) -> bool {
    periods.is_empty()
        || periods
            .iter()
            .any(|p| p.start.is_none_or(|s| s <= t) && p.end.is_none_or(|e| t <= e))
}

impl ServiceAlert {
    pub fn in_force(&self, t: u64) -> bool {
        self.status == AlertStatus::Published && in_periods(&self.communication, t)
    }

    pub fn ended_by(&self, t: u64) -> bool {
        !self.communication.is_empty()
            && self
                .communication
                .iter()
                .all(|p| p.end.is_some_and(|e| e < t))
    }

    fn upcoming_at(&self, t: u64) -> bool {
        !in_periods(&self.communication, t)
            && self
                .communication
                .iter()
                .any(|p| p.start.is_some_and(|s| s > t))
    }
}

impl ServiceAlerts {
    /// Unchecked. A write passes [`ServiceAlerts::checked`].
    pub fn put(&self, id: &str, alert: ServiceAlert) -> ServiceAlerts {
        let mut next = self.clone();
        next.alerts.insert(id.to_string(), alert);
        next
    }

    pub fn end(&self, id: &str, at: u64) -> Option<ServiceAlerts> {
        let mut next = self.clone();
        let a = next.alerts.get_mut(id)?;
        let clip = |periods: &[AlertPeriod]| -> Vec<AlertPeriod> {
            periods
                .iter()
                .filter(|p| p.start.is_none_or(|s| s <= at))
                .map(|p| AlertPeriod {
                    start: p.start,
                    end: Some(p.end.map_or(at, |e| e.min(at))),
                })
                .collect()
        };
        if a.communication.is_empty() {
            a.communication = vec![AlertPeriod {
                start: None,
                end: Some(at),
            }];
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

    pub fn list(&self, now: u64) -> AlertList {
        let mut list = AlertList::default();
        for (id, a) in &self.alerts {
            let row = |open_secs| AlertRow {
                id: id.clone(),
                alert: a.clone(),
                open_secs,
            };
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

    pub fn checked(self) -> Result<Self, Vec<AlertRefusal>> {
        let refusals: Vec<AlertRefusal> = self
            .alerts
            .iter()
            .flat_map(|(id, a)| {
                check(a).into_iter().map(move |problem| AlertRefusal {
                    alert: id.clone(),
                    problem,
                })
            })
            .chain(
                self.alerts
                    .keys()
                    .filter(|k| k.trim().is_empty())
                    .map(|_| AlertRefusal {
                        alert: String::new(),
                        problem: AlertProblem::NoId,
                    }),
            )
            .collect();
        if refusals.is_empty() {
            Ok(self)
        } else {
            Err(refusals)
        }
    }

    pub fn flags(
        &self,
        network: &transit_network::Network,
        languages: &[String],
    ) -> Vec<AlertFlag> {
        let mut out = Vec::new();
        for (id, a) in self
            .alerts
            .iter()
            .filter(|(_, a)| a.status != AlertStatus::Retired)
        {
            let mut push = |notice| {
                out.push(AlertFlag {
                    alert: id.clone(),
                    notice,
                })
            };
            for e in &a.informed {
                if let Some(r) = &e.route {
                    match network.routes.get(r) {
                        None => push(AlertNotice::UnknownRoute { route: r.clone() }),
                        Some(route) => {
                            if let Some(d) = e
                                .direction
                                .as_ref()
                                .filter(|d| !route.directions.contains_key(*d))
                            {
                                push(AlertNotice::UnknownDirection {
                                    route: r.clone(),
                                    direction: d.clone(),
                                });
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
            for (field, texts) in [
                ("header", &a.header),
                ("description", &a.description),
                ("url", &a.url),
            ] {
                if field != "header" && texts.is_empty() {
                    continue;
                }
                for lang in languages {
                    if !texts.iter().any(|t| {
                        t.lang.as_deref() == Some(lang.as_str()) && !t.text.trim().is_empty()
                    }) {
                        push(AlertNotice::MissingTranslation {
                            field: field.into(),
                            lang: lang.clone(),
                        });
                    }
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct AlertList {
    pub in_force: Vec<AlertRow>,
    pub upcoming: Vec<AlertRow>,
    pub ended: Vec<AlertRow>,
    pub drafts: Vec<AlertRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct AlertRow {
    pub id: String,
    pub alert: ServiceAlert,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub open_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct AlertRefusal {
    pub alert: String,
    pub problem: AlertProblem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlertProblem {
    NoId,
    NoEntity,
    NoHeader,
    EndBeforeStart,
    ImpactOutsideCommunication,
    DirectionWithoutRoute,
    InstanceWithoutTrip,
    BadDate { date: String },
    RepeatedLanguage { field: String, lang: Option<String> },
}

impl fmt::Display for AlertRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match &self.problem {
            AlertProblem::NoId => "has no id".to_string(),
            AlertProblem::NoEntity => {
                "is about nothing: it names no route, stop, trip, or agency".into()
            }
            AlertProblem::NoHeader => "has no header".into(),
            AlertProblem::EndBeforeStart => "has a period that ends before it starts".into(),
            AlertProblem::ImpactOutsideCommunication => {
                "has an impact period outside every communication period".into()
            }
            AlertProblem::DirectionWithoutRoute => "names a direction with no route".into(),
            AlertProblem::InstanceWithoutTrip => "names a trip date or start with no trip".into(),
            AlertProblem::BadDate { date } => format!("has {date:?}, not a YYYY-MM-DD date"),
            AlertProblem::RepeatedLanguage { field, lang } => format!(
                "has two {field} texts in {}",
                lang.as_deref().unwrap_or("the default language")
            ),
        };
        write!(f, "alert {:?} {what}", self.alert)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct AlertFlag {
    pub alert: String,
    pub notice: AlertNotice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlertNotice {
    UnknownRoute { route: String },
    UnknownDirection { route: String, direction: String },
    UnknownStop { stop: String },
    UnknownTrip { trip: String },
    MissingTranslation { field: String, lang: String },
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
            out.push(AlertProblem::RepeatedLanguage {
                field: field.into(),
                lang: t.lang.clone(),
            });
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
    } else if !a.communication.is_empty()
        && a.impact
            .iter()
            .any(|i| !a.communication.iter().any(|c| inside(i, c)))
    {
        out.push(AlertProblem::ImpactOutsideCommunication);
    }
    for e in &a.informed {
        if e.direction.is_some() && e.route.is_none() {
            out.push(AlertProblem::DirectionWithoutRoute);
        }
        if e.trip.is_none() && (e.date.is_some() || e.start_secs.is_some()) {
            out.push(AlertProblem::InstanceWithoutTrip);
        }
        if let Some(d) = e.date.as_ref().filter(|d| Date::parse(d).ok().is_none()) {
            out.push(AlertProblem::BadDate { date: d.clone() });
        }
    }
    for source in &a.explains {
        if let AlertSource::Trip { trip, date, .. } = source {
            if trip.trim().is_empty() {
                out.push(AlertProblem::InstanceWithoutTrip);
            }
            if Date::parse(date).ok().is_none() {
                out.push(AlertProblem::BadDate { date: date.clone() });
            }
        }
    }
    repeated_languages("header", &a.header, &mut out);
    repeated_languages("description", &a.description, &mut out);
    repeated_languages("url", &a.url, &mut out);
    out
}

pub const BLANKS: [&str; 4] = ["stop", "route", "start date", "end date"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct AlertTemplates {
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"alert_templates\""))]
    pub kind: AlertTemplatesKind,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<1>,
    pub templates: BTreeMap<String, AlertTemplate>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
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
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<AlertText>>"))]
    pub description: Vec<AlertText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<AlertText>>"))]
    pub url: Vec<AlertText>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<bool>"))]
    pub retired: bool,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct TemplateRefusal {
    pub template: String,
    pub problem: TemplateProblem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemplateProblem {
    NoId,
    NoName,
    NoHeader,
    UnknownBlank { blank: String },
    RepeatedLanguage { field: String, lang: Option<String> },
}

impl fmt::Display for TemplateRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match &self.problem {
            TemplateProblem::NoId => "has no id".to_string(),
            TemplateProblem::NoName => "has no name".into(),
            TemplateProblem::NoHeader => "has no header".into(),
            TemplateProblem::UnknownBlank { blank } => format!(
                "has the blank {{{blank}}}, which is not one of {}",
                BLANKS.map(|b| format!("{{{b}}}")).join(", ")
            ),
            TemplateProblem::RepeatedLanguage { field, lang } => format!(
                "has two {field} texts in {}",
                lang.as_deref().unwrap_or("the default language")
            ),
        };
        write!(f, "template {:?} {what}", self.template)
    }
}

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct TemplateRow {
    pub id: String,
    pub name: String,
    pub cause: AlertCause,
    pub effect: AlertEffect,
    pub severity: AlertSeverity,
}

impl AlertTemplates {
    pub fn offered(&self) -> Vec<TemplateRow> {
        let mut rows: Vec<TemplateRow> = self
            .templates
            .iter()
            .filter(|(_, t)| !t.retired)
            .map(|(id, t)| TemplateRow {
                id: id.clone(),
                name: t.name.clone(),
                cause: t.cause,
                effect: t.effect,
                severity: t.severity,
            })
            .collect();
        rows.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
        rows
    }

    /// Unchecked. A write passes [`AlertTemplates::checked`].
    pub fn put(&self, id: &str, template: AlertTemplate) -> AlertTemplates {
        let mut next = self.clone();
        next.templates.insert(id.to_string(), template);
        next
    }

    pub fn checked(self) -> Result<Self, Vec<TemplateRefusal>> {
        let mut refusals = Vec::new();
        for (id, t) in &self.templates {
            let mut push = |problem| {
                refusals.push(TemplateRefusal {
                    template: id.clone(),
                    problem,
                })
            };
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
            let mut unknown: Vec<&str> = texts
                .flat_map(|x| blanks_in(&x.text))
                .filter(|b| !BLANKS.contains(b))
                .collect();
            unknown.dedup();
            for blank in unknown {
                push(TemplateProblem::UnknownBlank {
                    blank: blank.to_string(),
                });
            }
            let mut problems = Vec::new();
            for (field, texts) in [
                ("header", &t.header),
                ("description", &t.description),
                ("url", &t.url),
            ] {
                let mut seen = std::collections::BTreeSet::new();
                for x in texts {
                    if !seen.insert(x.lang.clone()) {
                        problems.push(TemplateProblem::RepeatedLanguage {
                            field: field.into(),
                            lang: x.lang.clone(),
                        });
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

/// Dates are "YYYY-MM-DD", the host's, in the agency's zone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct FillInput {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<String>>"))]
    pub stops: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<String>>"))]
    pub routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<AlertEntity>>"))]
    pub informed: Vec<AlertEntity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub start_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub end_date: Option<String>,
}

impl FillInput {
    /// The input's stops, then the informed entities' stops, each once.
    fn stop_ids(&self) -> Vec<String> {
        unique(
            self.stops
                .iter()
                .cloned()
                .chain(self.informed.iter().filter_map(|e| e.stop.clone())),
        )
    }

    /// The input's routes, then the informed entities' routes, each once.
    fn route_ids(&self) -> Vec<String> {
        unique(
            self.routes
                .iter()
                .cloned()
                .chain(self.informed.iter().filter_map(|e| e.route.clone())),
        )
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Filled {
    pub cause: AlertCause,
    pub effect: AlertEffect,
    pub severity: AlertSeverity,
    pub header: Vec<AlertText>,
    pub description: Vec<AlertText>,
    pub url: Vec<AlertText>,
    pub unfilled: Vec<Unfilled>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Unfilled {
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub lang: Option<String>,
    pub blank: String,
}

/// The fill writes dates and lists in English only. No language is the
/// agency's default, taken as English.
fn english(lang: Option<&str>) -> bool {
    lang.is_none_or(|l| l == "en" || l.starts_with("en-"))
}

fn english_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

fn english_date(d: Date) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let weekday = match d.weekday() {
        transit_date::Weekday::Mon => "Monday",
        transit_date::Weekday::Tue => "Tuesday",
        transit_date::Weekday::Wed => "Wednesday",
        transit_date::Weekday::Thu => "Thursday",
        transit_date::Weekday::Fri => "Friday",
        transit_date::Weekday::Sat => "Saturday",
        transit_date::Weekday::Sun => "Sunday",
    };
    format!("{weekday}, {} {}", MONTHS[d.month0() as usize], d.day())
}

fn value(
    blank: &str,
    lang: Option<&str>,
    network: &transit_network::Network,
    input: &FillInput,
) -> Option<String> {
    let names = |ids: &[String], name: &dyn Fn(&str) -> Option<String>| -> Option<String> {
        if ids.is_empty() || (ids.len() > 1 && !english(lang)) {
            return None;
        }
        let names: Option<Vec<String>> = ids.iter().map(|id| name(id)).collect();
        Some(english_list(&names?))
    };
    let date = |d: &Option<String>| -> Option<String> {
        let d = Date::parse(d.as_deref()?).ok()?;
        english(lang).then(|| english_date(d))
    };
    match blank {
        "stop" => names(&input.stop_ids(), &|id| {
            network.stops.get(id).map(|s| s.name.clone())
        }),
        "route" => names(&input.route_ids(), &|id| {
            network.routes.get(id).map(|r| r.label().to_string())
        }),
        "start date" => date(&input.start_date),
        "end date" => date(&input.end_date),
        _ => None,
    }
}

pub fn fill(
    template: &AlertTemplate,
    network: &transit_network::Network,
    input: &FillInput,
) -> Filled {
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
                unfilled.extend(left.into_iter().map(|blank| Unfilled {
                    field: field.into(),
                    lang: t.lang.clone(),
                    blank,
                }));
                AlertText {
                    lang: t.lang.clone(),
                    text: out,
                }
            })
            .collect()
    };
    let header = texts("header", &template.header);
    let description = texts("description", &template.description);
    let url = texts("url", &template.url);
    Filled {
        cause: template.cause,
        effect: template.effect,
        severity: template.severity,
        header,
        description,
        url,
        unfilled,
    }
}
