#![allow(deprecated)]
//! Service alerts at the boundary: the Alerts entities of an rt
//! snapshot decode into the overview, and the live document keeps the
//! ones IN FORCE at the snapshot's moment (its communication period,
//! else the deprecated active period; none = always), each informed
//! entity resolved into the network's routes, stops, and trip
//! instances. A selector naming something the network does not have
//! resolves to nothing; an alert none of whose selectors resolve is
//! counted, never silently dropped.
//!
//! Over the e2e fixture network: route 10 (bus), stops A B C, trip t10
//! at 08:00.

use prost::Message;
use transit_codec::Boundary;
use transit_live::{AlertCause, AlertEffect, AlertScope, OnTimeWindow};
use transit_rt::gtfs_rt::{
    alert, translated_string::Translation, Alert, EntitySelector, FeedEntity, FeedHeader, FeedMessage, TimeRange,
    TranslatedString, TripDescriptor,
};
use transit_rt::{decode_overview, live_day};

const NOW: u64 = 1_800_000_000;

fn network() -> (transit_network::Network, Boundary) {
    let feed = transit_codec::Feed::from_zip_bytes(include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    let imported = transit_codec::import(&feed);
    (imported.network, imported.boundary)
}

fn text(s: &str) -> Option<TranslatedString> {
    Some(TranslatedString { translation: vec![Translation { text: s.into(), language: None }] })
}

fn selector() -> EntitySelector {
    EntitySelector::default()
}

fn alert_entity(id: &str, informed: Vec<EntitySelector>, communication: Vec<TimeRange>) -> FeedEntity {
    FeedEntity {
        id: id.into(),
        alert: Some(Alert {
            communication_period: communication,
            informed_entity: informed,
            cause: Some(alert::Cause::Construction as i32),
            effect: Some(alert::Effect::Detour as i32),
            header_text: text("Route 10 detour"),
            description_text: text("Stop C is closed."),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn snapshot(entities: Vec<FeedEntity>) -> transit_rt::RtOverview {
    let msg = FeedMessage {
        header: FeedHeader { gtfs_realtime_version: "2.0".into(), timestamp: Some(NOW), ..Default::default() },
        entity: entities,
    };
    decode_overview(&msg.encode_to_vec()).unwrap()
}

fn alerts_of(rt: &transit_rt::RtOverview) -> transit_live::LiveDay {
    let (net, boundary) = network();
    live_day(&net, &boundary, None, None, Some(rt), OnTimeWindow::default())
}

#[test]
fn an_alert_decodes_and_resolves_into_model_vocabulary() {
    let rt = snapshot(vec![alert_entity(
        "a1",
        vec![EntitySelector { route_id: Some("10".into()), stop_id: Some("C".into()), ..selector() }],
        vec![],
    )]);
    assert_eq!(rt.alerts, 1);
    assert_eq!(rt.service_alerts.len(), 1);
    let day = alerts_of(&rt);
    assert_eq!(day.unmatched_alerts, 0);
    let a = &day.alerts[0];
    assert_eq!((a.id.as_str(), a.cause, a.effect), ("a1", AlertCause::Construction, AlertEffect::Detour));
    assert_eq!(a.header, "Route 10 detour");
    assert_eq!(a.description.as_deref(), Some("Stop C is closed."));
    assert_eq!(a.scopes, vec![AlertScope { route: Some("10".into()), stop: Some("C".into()), ..Default::default() }]);
}

#[test]
fn only_alerts_in_force_at_the_snapshot_moment_are_kept() {
    let period = |start: u64, end: u64| TimeRange { start: Some(start), end: Some(end) };
    let route = || vec![EntitySelector { route_id: Some("10".into()), ..selector() }];
    let rt = snapshot(vec![
        alert_entity("now", route(), vec![period(NOW - 60, NOW + 60)]),
        alert_entity("later", route(), vec![period(NOW + 3600, NOW + 7200)]),
        alert_entity("past", route(), vec![period(NOW - 7200, NOW - 3600)]),
        alert_entity("open", route(), vec![TimeRange { start: Some(NOW - 60), end: None }]),
        alert_entity("always", route(), vec![]),
    ]);
    let ids: Vec<_> = alerts_of(&rt).alerts.into_iter().map(|a| a.id).collect();
    assert_eq!(ids, ["now", "open", "always"]);
}

#[test]
fn the_deprecated_active_period_counts_when_no_communication_period_is_given() {
    let mut entity = alert_entity("a", vec![EntitySelector { route_id: Some("10".into()), ..selector() }], vec![]);
    entity.alert.as_mut().unwrap().active_period = vec![TimeRange { start: Some(NOW + 60), end: None }];
    assert!(alerts_of(&snapshot(vec![entity])).alerts.is_empty());
}

#[test]
fn selectors_resolve_or_count() {
    let rt = snapshot(vec![
        // Agency-wide: every field absent.
        alert_entity("agency", vec![EntitySelector { agency_id: Some("any".into()), ..selector() }], vec![]),
        // A route_type names the routes published under it (the
        // fixture's routes are all buses, 3).
        alert_entity("buses", vec![EntitySelector { route_type: Some(3), ..selector() }], vec![]),
        alert_entity("ferries", vec![EntitySelector { route_type: Some(4), ..selector() }], vec![]),
        // A trip resolves to its instance.
        alert_entity(
            "trip",
            vec![EntitySelector {
                trip: Some(TripDescriptor { trip_id: Some("t10".into()), ..Default::default() }),
                ..selector()
            }],
            vec![],
        ),
        // One unknown selector among known ones: the known one stays.
        alert_entity(
            "mixed",
            vec![
                EntitySelector { stop_id: Some("nowhere".into()), ..selector() },
                EntitySelector { stop_id: Some("A".into()), ..selector() },
            ],
            vec![],
        ),
        // Nothing resolves: counted.
        alert_entity("ghost", vec![EntitySelector { route_id: Some("99".into()), ..selector() }], vec![]),
        alert_entity("ghost-stop", vec![EntitySelector { route_id: Some("10".into()), stop_id: Some("Z".into()), ..selector() }], vec![]),
    ]);
    let day = alerts_of(&rt);
    let scopes = |id: &str| day.alerts.iter().find(|a| a.id == id).map(|a| a.scopes.clone());
    assert_eq!(scopes("agency"), Some(vec![AlertScope::default()]));
    let bus_routes: Vec<_> = network().0.routes.keys().map(|r| AlertScope { route: Some(r.clone()), ..Default::default() }).collect();
    assert!(bus_routes.len() > 1);
    assert_eq!(scopes("buses"), Some(bus_routes));
    assert_eq!(scopes("ferries"), None);
    assert_eq!(
        scopes("trip"),
        Some(vec![AlertScope { trip: Some("t10".into()), start_secs: Some(8 * 3600), ..Default::default() }])
    );
    assert_eq!(scopes("mixed"), Some(vec![AlertScope { stop: Some("A".into()), ..Default::default() }]));
    assert_eq!(scopes("ghost"), None);
    assert_eq!(day.unmatched_alerts, 3, "ferries, ghost, ghost-stop");
}

#[test]
fn alerts_from_several_snapshots_are_one_set() {
    let entity = || alert_entity("a1", vec![EntitySelector { route_id: Some("10".into()), ..selector() }], vec![]);
    let (net, boundary) = network();
    let rt = snapshot(vec![entity()]);
    let alerts_feed = snapshot(vec![entity()]);
    // A combined feed's own alerts count; the same entity id is one alert.
    let day = live_day(&net, &boundary, Some(&rt), None, Some(&alerts_feed), OnTimeWindow::default());
    assert_eq!(day.alerts.len(), 1);
}
