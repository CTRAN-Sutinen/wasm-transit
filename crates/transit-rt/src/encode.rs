use prost::Message;
use serde::{Deserialize, Serialize};
use transit_alerts::{AlertCause, AlertEffect, AlertPeriod, AlertSeverity};
use transit_alerts::{AlertEntity, AlertStatus, AlertText, ServiceAlerts};
use transit_network::Network;
#[cfg(feature = "bindings")]
use ts_rs::TS;

use crate::gtfs_rt::{
    alert, feed_header::Incrementality, translated_string::Translation, Alert, EntitySelector,
    FeedEntity, FeedHeader, FeedMessage, TimeRange, TranslatedString, TripDescriptor,
};

#[derive(Debug, Clone, PartialEq)]
pub struct EncodedAlerts {
    pub bytes: Vec<u8>,
    pub skipped: Vec<SkippedEntity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct SkippedEntity {
    pub alert: String,
    pub entity: u32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityReading {
    pub published: Result<crate::overview::InformedView, String>,
    pub scopes: Vec<transit_live::AlertScope>,
    pub names: EntityNames,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct EntityNames {
    pub agency: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub route: Option<EntityRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub route_type: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub direction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub stop: Option<EntityStop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub trip: Option<EntityTrip>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct EntityRoute {
    pub id: String,
    pub name: String,
    pub known: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct EntityStop {
    pub id: String,
    pub name: String,
    pub known: bool,
    pub bays: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct EntityTrip {
    pub id: String,
    pub known: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub start_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub headsign: Option<String>,
}

fn names(network: &Network, e: &AlertEntity, scopes: &[transit_live::AlertScope]) -> EntityNames {
    let route_name = |id: &str| network.routes.get(id).map(|r| r.label().to_string());
    EntityNames {
        agency: e.route.is_none() && e.route_type.is_none() && e.stop.is_none() && e.trip.is_none(),
        route: e.route.as_ref().map(|id| EntityRoute {
            id: id.clone(),
            name: route_name(id).unwrap_or_else(|| id.clone()),
            known: network.routes.contains_key(id),
        }),
        route_type: e.route_type,
        direction: e.direction.clone(),
        stop: e.stop.as_ref().map(|id| EntityStop {
            id: id.clone(),
            name: network
                .stops
                .get(id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| id.clone()),
            known: network.stops.contains_key(id),
            bays: network
                .stops
                .values()
                .filter(|s| s.parent.as_deref() == Some(id.as_str()))
                .count() as u32,
        }),
        trip: e.trip.as_ref().map(|id| {
            let found = network.trip(id);
            let pattern = found.and_then(|(_, pt, _)| network.patterns.get(&pt.pattern));
            EntityTrip {
                id: id.clone(),
                known: found.is_some(),
                route: pattern.and_then(|p| route_name(&p.route)),
                start_secs: e
                    .start_secs
                    .or_else(|| scopes.first().and_then(|s| s.start_secs)),
                date: e.date.clone(),
                headsign: found.and_then(|(_, _, t)| t.headsign.clone()),
            }
        }),
    }
}

/// Runs the entity through the encoder's selector and then the reader's
/// resolution, so it shows what a rider's app reads.
pub fn entity_reading(network: &Network, e: &AlertEntity) -> EntityReading {
    let agency = network.operators.keys().next().cloned().unwrap_or_default();
    match selector(network, &agency, e) {
        Ok(s) => {
            let view = crate::overview::informed_view(&s);
            let scopes = crate::live::resolve_scope(network, &view);
            EntityReading {
                names: names(network, e, &scopes),
                published: Ok(view),
                scopes,
            }
        }
        Err(reason) => EntityReading {
            names: names(network, e, &[]),
            published: Err(reason),
            scopes: Vec::new(),
        },
    }
}

pub fn encode_alerts(network: &Network, alerts: &ServiceAlerts, timestamp: u64) -> EncodedAlerts {
    let agency = network.operators.keys().next().cloned().unwrap_or_default();
    let mut skipped = Vec::new();
    let mut entity = Vec::new();
    for (id, a) in &alerts.alerts {
        if a.status != AlertStatus::Published || a.ended_by(timestamp) {
            continue;
        }
        let mut informed = Vec::new();
        for (i, e) in a.informed.iter().enumerate() {
            match selector(network, &agency, e) {
                Ok(s) => informed.push(s),
                Err(reason) => skipped.push(SkippedEntity {
                    alert: id.clone(),
                    entity: i as u32,
                    reason,
                }),
            }
        }
        if informed.is_empty() {
            continue;
        }
        entity.push(FeedEntity {
            id: id.clone(),
            alert: Some(Alert {
                communication_period: ranges(&a.communication),
                impact_period: ranges(&a.impact),
                informed_entity: informed,
                cause: (a.cause != AlertCause::Unknown).then(|| cause(a.cause) as i32),
                effect: (a.effect != AlertEffect::Unknown).then(|| effect(a.effect) as i32),
                severity_level: (a.severity != AlertSeverity::Unknown)
                    .then(|| severity(a.severity) as i32),
                header_text: translated(&a.header),
                description_text: translated(&a.description),
                url: translated(&a.url),
                ..Default::default()
            }),
            ..Default::default()
        });
    }
    let message = FeedMessage {
        header: FeedHeader {
            gtfs_realtime_version: "2.0".into(),
            incrementality: Some(Incrementality::FullDataset as i32),
            timestamp: Some(timestamp),
            ..Default::default()
        },
        entity,
    };
    EncodedAlerts {
        bytes: message.encode_to_vec(),
        skipped,
    }
}

fn ranges(periods: &[AlertPeriod]) -> Vec<TimeRange> {
    periods
        .iter()
        .map(|p| TimeRange {
            start: p.start,
            end: p.end,
        })
        .collect()
}

fn translated(texts: &[AlertText]) -> Option<TranslatedString> {
    (!texts.is_empty()).then(|| TranslatedString {
        translation: texts
            .iter()
            .map(|t| Translation {
                text: t.text.clone(),
                language: t.lang.clone(),
            })
            .collect(),
    })
}

/// Past 24:00:00 for a trip after midnight.
fn clock(secs: u32) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn selector(network: &Network, agency: &str, e: &AlertEntity) -> Result<EntitySelector, String> {
    let whole_agency =
        e.route.is_none() && e.route_type.is_none() && e.stop.is_none() && e.trip.is_none();
    let direction_id = match (&e.direction, &e.route) {
        (None, _) => None,
        (Some(_), None) => return Err("a direction needs a route".into()),
        (Some(d), Some(r)) => {
            let route = network.routes.get(r).ok_or_else(|| {
                format!("route {r:?} is not in the network, so direction {d:?} has no code")
            })?;
            let code = transit_codec::gtfs_direction(route, d)
                .ok_or_else(|| format!("route {r:?} direction {d:?} has no GTFS direction_id"))?;
            Some(u32::from(code))
        }
    };
    let trip = e.trip.as_ref().map(|t| TripDescriptor {
        trip_id: Some(t.clone()),
        start_date: e.date.as_ref().map(|d| d.replace('-', "")),
        start_time: e.start_secs.map(clock),
        ..Default::default()
    });
    Ok(EntitySelector {
        agency_id: whole_agency.then(|| agency.to_string()),
        route_id: e.route.clone(),
        route_type: e.route_type.map(i32::from),
        trip,
        stop_id: e.stop.clone(),
        direction_id,
    })
}

fn cause(c: AlertCause) -> alert::Cause {
    use alert::Cause as P;
    match c {
        AlertCause::Unknown => P::UnknownCause,
        AlertCause::Other => P::OtherCause,
        AlertCause::TechnicalProblem => P::TechnicalProblem,
        AlertCause::Strike => P::Strike,
        AlertCause::Demonstration => P::Demonstration,
        AlertCause::Accident => P::Accident,
        AlertCause::Holiday => P::Holiday,
        AlertCause::Weather => P::Weather,
        AlertCause::Maintenance => P::Maintenance,
        AlertCause::Construction => P::Construction,
        AlertCause::PoliceActivity => P::PoliceActivity,
        AlertCause::MedicalEmergency => P::MedicalEmergency,
        AlertCause::SpecialEvent => P::SpecialEvent,
    }
}

fn effect(e: AlertEffect) -> alert::Effect {
    use alert::Effect as P;
    match e {
        AlertEffect::NoService => P::NoService,
        AlertEffect::ReducedService => P::ReducedService,
        AlertEffect::SignificantDelays => P::SignificantDelays,
        AlertEffect::Detour => P::Detour,
        AlertEffect::AdditionalService => P::AdditionalService,
        AlertEffect::ModifiedService => P::ModifiedService,
        AlertEffect::Other => P::OtherEffect,
        AlertEffect::Unknown => P::UnknownEffect,
        AlertEffect::StopMoved => P::StopMoved,
        AlertEffect::NoEffect => P::NoEffect,
        AlertEffect::AccessibilityIssue => P::AccessibilityIssue,
    }
}

fn severity(s: AlertSeverity) -> alert::SeverityLevel {
    use alert::SeverityLevel as P;
    match s {
        AlertSeverity::Unknown => P::UnknownSeverity,
        AlertSeverity::Info => P::Info,
        AlertSeverity::Warning => P::Warning,
        AlertSeverity::Severe => P::Severe,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadAlerts {
    pub alerts: ServiceAlerts,
    pub skipped: Vec<SkippedEntity>,
    pub dropped: Vec<DroppedAlert>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct DroppedAlert {
    pub alert: String,
    pub reason: String,
}

pub fn read_alerts(network: &Network, bytes: &[u8]) -> crate::Result<ReadAlerts> {
    let message = crate::decode(bytes)?;
    let mut doc = ServiceAlerts::default();
    let (mut skipped, mut dropped) = (Vec::new(), Vec::new());
    for entity in &message.entity {
        let Some(alert) = &entity.alert else { continue };
        let id = entity.id.clone();
        let view = crate::overview::alert_view(&id, alert);
        let mut informed = Vec::new();
        for (i, e) in alert.informed_entity.iter().enumerate() {
            match read_selector(network, e) {
                Ok(a) => informed.push(a),
                Err(reason) => skipped.push(SkippedEntity {
                    alert: id.clone(),
                    entity: i as u32,
                    reason,
                }),
            }
        }
        let header = texts(alert.header_text.as_ref());
        if informed.is_empty() {
            dropped.push(DroppedAlert {
                alert: id,
                reason: "no informed entity could be read".into(),
            });
            continue;
        }
        if !header.iter().any(|t| !t.text.trim().is_empty()) {
            dropped.push(DroppedAlert {
                alert: id,
                reason: "no header text".into(),
            });
            continue;
        }
        doc.alerts.insert(
            id,
            transit_alerts::ServiceAlert {
                status: AlertStatus::Published,
                cause: view.cause,
                effect: view.effect,
                severity: view.severity,
                header,
                description: texts(alert.description_text.as_ref()),
                url: texts(alert.url.as_ref()),
                communication: view.communication,
                impact: view.impact,
                informed,
                ..Default::default()
            },
        );
    }
    Ok(ReadAlerts {
        alerts: doc,
        skipped,
        dropped,
    })
}

fn texts(text: Option<&TranslatedString>) -> Vec<AlertText> {
    text.map(|t| {
        t.translation
            .iter()
            .map(|x| AlertText {
                lang: x.language.clone().filter(|l| !l.is_empty()),
                text: x.text.clone(),
            })
            .collect()
    })
    .unwrap_or_default()
}

fn read_selector(network: &Network, e: &EntitySelector) -> Result<AlertEntity, String> {
    let route_type = match e.route_type {
        None => None,
        Some(t) => {
            Some(u16::try_from(t).map_err(|_| format!("route_type {t} is not a GTFS route type"))?)
        }
    };
    let direction = match (e.direction_id, &e.route_id) {
        (None, _) => None,
        (Some(_), None) => {
            return Err("a direction_id with no route_id names nothing (the GTFS-rt spec)".into())
        }
        (Some(code), Some(r)) => {
            let route = network.routes.get(r).ok_or_else(|| {
                format!("route {r:?} is not in the network, so direction_id {code} names nothing")
            })?;
            Some(
                transit_codec::model_direction(route, code)
                    .ok_or_else(|| {
                        format!("route {r:?} has no direction with direction_id {code}")
                    })?
                    .clone(),
            )
        }
    };
    let (trip, date, start_secs) = match &e.trip {
        None => (None, None, None),
        Some(t) => {
            let trip = t
                .trip_id
                .clone()
                .ok_or("a trip with no trip_id names nothing")?;
            let date = match &t.start_date {
                None => None,
                Some(d) => Some(
                    transit_codec::parse_gtfs_date(d)
                        .filter(|_| d.trim() == d)
                        .ok_or_else(|| format!("start_date {d:?} is not YYYYMMDD"))?
                        .to_string(),
                ),
            };
            let start = match &t.start_time {
                None => None,
                Some(s) => Some(
                    crate::progress::parse_time(s)
                        .ok_or_else(|| format!("start_time {s:?} is not HH:MM:SS"))?,
                ),
            };
            (Some(trip), date, start)
        }
    };
    let entity = AlertEntity {
        route: e.route_id.clone(),
        route_type,
        direction,
        stop: e.stop_id.clone(),
        trip,
        date,
        start_secs,
        ..Default::default()
    };
    if entity == AlertEntity::default() && e.agency_id.is_none() {
        return Err("the entity names nothing".into());
    }
    Ok(entity)
}
