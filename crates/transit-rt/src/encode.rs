//! The service alerts document written as a GTFS-rt Alerts feed.
//!
//! GTFS-rt is the alerts' serialization, as GTFS is the network's. The
//! feed is a full dataset of the published alerts not yet ended at the
//! feed's moment, one entity for each, under the alert's id, in id
//! order. The bytes are the same for the same document and moment.
//!
//! An entity is written as authored, in the network's ids. A direction
//! is written as the GTFS code the export gives it
//! (`transit_codec::gtfs_direction`). A direction with no code cannot
//! be written as the author meant it, so the entity is left out and
//! named in [`EncodedAlerts::skipped`]. It is never widened to the
//! whole route. An alert left with no entity is not written.

use prost::Message;
use serde::{Deserialize, Serialize};
use transit_alerts::{AlertEntity, AlertStatus, AlertText, ServiceAlerts};
use transit_live::{AlertCause, AlertEffect, AlertPeriod, AlertSeverity};
use transit_network::Network;
use ts_rs::TS;

use crate::gtfs_rt::{
    alert, feed_header::Incrementality, translated_string::Translation, Alert, EntitySelector, FeedEntity, FeedHeader,
    FeedMessage, TimeRange, TranslatedString, TripDescriptor,
};

/// The encoded feed, and each entity it could not write.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodedAlerts {
    pub bytes: Vec<u8>,
    pub skipped: Vec<SkippedEntity>,
}

/// An informed entity left out of the feed: the alert, the entity's
/// index in its `informed` list, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkippedEntity {
    pub alert: String,
    pub entity: u32,
    pub reason: String,
}

/// The published alerts not yet ended at `timestamp` (Unix seconds),
/// as a GTFS-rt `FeedMessage`.
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
                Err(reason) => skipped.push(SkippedEntity { alert: id.clone(), entity: i as u32, reason }),
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
                severity_level: (a.severity != AlertSeverity::Unknown).then(|| severity(a.severity) as i32),
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
    EncodedAlerts { bytes: message.encode_to_vec(), skipped }
}

fn ranges(periods: &[AlertPeriod]) -> Vec<TimeRange> {
    periods.iter().map(|p| TimeRange { start: p.start, end: p.end }).collect()
}

fn translated(texts: &[AlertText]) -> Option<TranslatedString> {
    (!texts.is_empty()).then(|| TranslatedString {
        translation: texts.iter().map(|t| Translation { text: t.text.clone(), language: t.lang.clone() }).collect(),
    })
}

/// "HH:MM:SS"; past 24:00:00 for a trip after midnight, as GTFS writes
/// it.
fn clock(secs: u32) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn selector(network: &Network, agency: &str, e: &AlertEntity) -> Result<EntitySelector, String> {
    let whole_agency = e.route.is_none() && e.route_type.is_none() && e.stop.is_none() && e.trip.is_none();
    let direction_id = match (&e.direction, &e.route) {
        (None, _) => None,
        (Some(_), None) => return Err("a direction needs a route".into()),
        (Some(d), Some(r)) => {
            let route = network.routes.get(r).ok_or_else(|| format!("route {r:?} is not in the network, so direction {d:?} has no code"))?;
            let code = transit_codec::gtfs_direction(route, d).ok_or_else(|| format!("route {r:?} direction {d:?} has no GTFS direction_id"))?;
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
