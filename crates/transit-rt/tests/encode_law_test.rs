//! The service alerts document encoded as a GTFS-rt Alerts feed. The
//! feed is the alerts' serialization: decoding it again and building
//! the live document gives back the authored alerts in force, in the
//! network's ids. The bytes are the same for the same document and
//! moment.
//!
//! Over the e2e fixture network: routes 10, 20, 40 (route 40 runs
//! directions 0 and 1), stops A B C SA SB, trip t10 at 08:00.

use prost::Message;
use proptest::prelude::*;
use transit_alerts::{AlertEntity, AlertStatus, AlertText, ServiceAlert, ServiceAlerts};
use transit_codec::Boundary;
use transit_live::{AlertScope, LiveAlert, OnTimeWindow};
use transit_alerts::{AlertCause, AlertEffect, AlertPeriod, AlertSeverity};
use transit_network::{Departs, Network};
use transit_rt::{decode_overview, encode_alerts, live_day};

fn network() -> (Network, Boundary) {
    let feed = transit_codec::Feed::from_zip_bytes(include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    let imported = transit_codec::import(&feed);
    (imported.network, imported.boundary)
}

fn live_of(net: &Network, boundary: &Boundary, bytes: &[u8]) -> transit_live::LiveDay {
    let rt = decode_overview(bytes).unwrap();
    live_day(net, boundary, None, None, Some(&rt), OnTimeWindow::default())
}

/// The scope an authored entity resolves to, written from the entity
/// and the network: a trip that departs at one time names that
/// instance.
fn expected_scope(net: &Network, e: &AlertEntity) -> AlertScope {
    let start_secs = e.start_secs.or_else(|| {
        let (_, _, trip) = net.trip(e.trip.as_deref()?)?;
        match trip.departs {
            Departs::At { start_secs } => Some(start_secs),
            _ => None,
        }
    });
    AlertScope {
        route: e.route.clone(),
        direction: e.direction.clone(),
        stop: e.stop.clone(),
        trip: e.trip.clone(),
        date: e.date.clone(),
        start_secs,
    }
}

fn first_text(texts: &[AlertText]) -> Option<String> {
    texts.iter().find(|t| t.lang.is_none()).or(texts.first()).map(|t| t.text.clone())
}

fn expected(net: &Network, id: &str, a: &ServiceAlert) -> LiveAlert {
    LiveAlert {
        id: id.into(),
        cause: a.cause,
        effect: a.effect,
        severity: a.severity,
        header: first_text(&a.header).unwrap_or_default(),
        description: first_text(&a.description),
        url: first_text(&a.url),
        impact: a.impact.clone(),
        scopes: a.informed.iter().map(|e| expected_scope(net, e)).collect(),
    }
}

// Entities over the fixture's own ids, every one of which resolves.
fn arb_entity() -> impl Strategy<Value = AlertEntity> {
    prop_oneof![
        prop::sample::select(vec!["10", "20", "40"]).prop_map(|r| AlertEntity { route: Some(r.into()), ..Default::default() }),
        prop::sample::select(vec!["0", "1"])
            .prop_map(|d| AlertEntity { route: Some("40".into()), direction: Some(d.into()), ..Default::default() }),
        prop::sample::select(vec!["A", "B", "C", "SA", "SB"]).prop_map(|s| AlertEntity { stop: Some(s.into()), ..Default::default() }),
        (prop::sample::select(vec!["10", "20"]), prop::sample::select(vec!["A", "B", "C"]))
            .prop_map(|(r, s)| AlertEntity { route: Some(r.into()), stop: Some(s.into()), ..Default::default() }),
        proptest::option::of(Just("2026-11-19"))
            .prop_map(|d| AlertEntity { trip: Some("t10".into()), date: d.map(String::from), ..Default::default() }),
    ]
}

fn arb_period() -> impl Strategy<Value = AlertPeriod> {
    (proptest::option::of(0u64..2000), proptest::option::of(0u64..2000)).prop_map(|(a, b)| match (a, b) {
        (Some(a), Some(b)) => AlertPeriod { start: Some(a.min(b)), end: Some(a.max(b)) },
        (a, b) => AlertPeriod { start: a, end: b },
    })
}

fn arb_texts() -> impl Strategy<Value = Vec<AlertText>> {
    prop::sample::subsequence(vec![None, Some("en"), Some("es"), Some("ru")], 1..4).prop_map(|langs| {
        langs.into_iter().map(|l| AlertText { lang: l.map(String::from), text: format!("Detour ({})", l.unwrap_or("default")) }).collect()
    })
}

fn arb_alert() -> impl Strategy<Value = ServiceAlert> {
    (
        prop::sample::select(vec![AlertStatus::Draft, AlertStatus::Published, AlertStatus::Published, AlertStatus::Retired]),
        prop::sample::select(vec![AlertCause::Unknown, AlertCause::Construction, AlertCause::Weather]),
        prop::sample::select(vec![AlertEffect::Unknown, AlertEffect::Detour, AlertEffect::NoService]),
        prop::sample::select(vec![AlertSeverity::Unknown, AlertSeverity::Warning]),
        arb_texts(),
        prop::collection::vec(arb_period(), 0..3),
        prop::collection::vec(arb_entity(), 1..4),
    )
        .prop_map(|(status, cause, effect, severity, header, communication, informed)| ServiceAlert {
            status,
            cause,
            effect,
            severity,
            description: header.clone(),
            header,
            communication,
            informed,
            ..Default::default()
        })
}

proptest! {
    /// Encode, decode, and build the live document at the feed's
    /// moment: the result is exactly the authored alerts in force then,
    /// with their scopes resolved, and none is unmatched.
    #[test]
    fn the_feed_reads_back_as_the_authored_alerts_in_force(
        alerts in prop::collection::btree_map("[a-d][0-9]", arb_alert(), 0..5),
        now in 0u64..2100,
    ) {
        let (net, boundary) = network();
        let doc = ServiceAlerts { alerts, ..Default::default() };
        let encoded = encode_alerts(&net, &doc, now);
        prop_assert!(encoded.skipped.is_empty());
        let day = live_of(&net, &boundary, &encoded.bytes);
        let want: Vec<LiveAlert> = doc.alerts.iter().filter(|(_, a)| a.in_force(now)).map(|(id, a)| expected(&net, id, a)).collect();
        prop_assert_eq!(day.unmatched_alerts, 0);
        prop_assert_eq!(day.alerts, want);
    }

    /// The same document and moment give the same bytes.
    #[test]
    fn the_bytes_are_the_same_for_the_same_document(
        alerts in prop::collection::btree_map("[a-d][0-9]", arb_alert(), 0..5),
        now in 0u64..2100,
    ) {
        let (net, _) = network();
        let doc = ServiceAlerts { alerts, ..Default::default() };
        let again: ServiceAlerts = serde_json::from_value(serde_json::to_value(&doc).unwrap()).unwrap();
        prop_assert_eq!(encode_alerts(&net, &doc, now).bytes, encode_alerts(&net, &again, now).bytes);
    }

    /// Every translation of every text survives the encode and the
    /// decode of the FeedMessage.
    #[test]
    fn every_translation_survives(alert in arb_alert()) {
        let (net, _) = network();
        let alert = ServiceAlert { status: AlertStatus::Published, communication: vec![], ..alert };
        let doc = ServiceAlerts::default().put("a1", alert.clone());
        let msg = transit_rt::gtfs_rt::FeedMessage::decode(encode_alerts(&net, &doc, 0).bytes.as_slice()).unwrap();
        let encoded = msg.entity[0].alert.as_ref().unwrap();
        let texts = |t: Option<&transit_rt::gtfs_rt::TranslatedString>| -> Vec<AlertText> {
            t.map(|t| t.translation.iter().map(|x| AlertText { lang: x.language.clone(), text: x.text.clone() }).collect()).unwrap_or_default()
        };
        prop_assert_eq!(texts(encoded.header_text.as_ref()), alert.header);
        prop_assert_eq!(texts(encoded.description_text.as_ref()), alert.description);
    }
}

#[test]
fn the_agency_and_a_route_type_round_trip() {
    let (net, boundary) = network();
    let mut a = ServiceAlert {
        status: AlertStatus::Published,
        header: vec![AlertText { lang: None, text: "Snow".into() }],
        informed: vec![AlertEntity::default()],
        ..Default::default()
    };
    let doc = ServiceAlerts::default().put("agency", a.clone());
    let day = live_of(&net, &boundary, &encode_alerts(&net, &doc, 0).bytes);
    assert_eq!(day.alerts[0].scopes, vec![AlertScope::default()]);
    a.informed = vec![AlertEntity { route_type: Some(3), ..Default::default() }];
    let doc = ServiceAlerts::default().put("buses", a);
    let day = live_of(&net, &boundary, &encode_alerts(&net, &doc, 0).bytes);
    let routes: Vec<_> = net.routes.keys().map(|r| AlertScope { route: Some(r.clone()), ..Default::default() }).collect();
    assert_eq!(day.alerts[0].scopes, routes);
}

/// The feed is a full dataset of the published alerts not yet ended,
/// one entity for each, under the alert's id.
#[test]
fn the_feed_holds_the_published_alerts_not_yet_ended() {
    let (net, _) = network();
    let alert = |status, end: Option<u64>| ServiceAlert {
        status,
        header: vec![AlertText { lang: None, text: "x".into() }],
        communication: vec![AlertPeriod { start: Some(0), end }],
        informed: vec![AlertEntity { route: Some("10".into()), ..Default::default() }],
        ..Default::default()
    };
    let doc = ServiceAlerts::default()
        .put("draft", alert(AlertStatus::Draft, None))
        .put("ended", alert(AlertStatus::Published, Some(50)))
        .put("later", ServiceAlert { communication: vec![AlertPeriod { start: Some(500), end: None }], ..alert(AlertStatus::Published, None) })
        .put("open", alert(AlertStatus::Published, None))
        .put("withdrawn", alert(AlertStatus::Retired, None));
    let msg = transit_rt::gtfs_rt::FeedMessage::decode(encode_alerts(&net, &doc, 100).bytes.as_slice()).unwrap();
    assert_eq!(msg.header.timestamp, Some(100));
    assert_eq!(msg.header.incrementality, Some(transit_rt::gtfs_rt::feed_header::Incrementality::FullDataset as i32));
    let ids: Vec<_> = msg.entity.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["later", "open"]);
}

/// A direction with no GTFS code (a route's third direction, or a
/// direction of a route the network does not have) cannot be written as
/// the author meant it: the entity is left out and named, never
/// widened. Any other unknown id is written as authored; the receive
/// path flags it for a person.
#[test]
fn an_entity_that_cannot_be_written_is_named_not_widened() {
    let (net, _) = network();
    let doc = ServiceAlerts::default().put(
        "a1",
        ServiceAlert {
            status: AlertStatus::Published,
            header: vec![AlertText { lang: None, text: "x".into() }],
            informed: vec![
                AlertEntity { route: Some("40".into()), direction: Some("2".into()), ..Default::default() },
                AlertEntity { route: Some("10".into()), ..Default::default() },
            ],
            ..Default::default()
        },
    );
    let encoded = encode_alerts(&net, &doc, 0);
    assert_eq!(encoded.skipped.len(), 1);
    assert_eq!((encoded.skipped[0].alert.as_str(), encoded.skipped[0].entity), ("a1", 0));
    let msg = transit_rt::gtfs_rt::FeedMessage::decode(encoded.bytes.as_slice()).unwrap();
    assert_eq!(msg.entity[0].alert.as_ref().unwrap().informed_entity.len(), 1);
}

// ---------------------------------------------------------------------
// Reading an Alerts feed into the document: the inverse of the encode.
// ---------------------------------------------------------------------

proptest! {
    /// Reading back an encoded document gives the published alerts not
    /// yet ended, each as authored but for what the feed does not carry
    /// (the disruptions it explains).
    #[test]
    fn reading_the_feed_back_gives_the_published_alerts(
        alerts in prop::collection::btree_map("[a-d][0-9]", arb_alert(), 0..5),
        now in 0u64..2100,
    ) {
        let (net, _) = network();
        let doc = ServiceAlerts { alerts, ..Default::default() };
        let read = transit_rt::read_alerts(&net, &encode_alerts(&net, &doc, now).bytes).unwrap();
        prop_assert!(read.skipped.is_empty() && read.dropped.is_empty());
        let want: std::collections::BTreeMap<String, ServiceAlert> = doc
            .alerts
            .iter()
            .filter(|(_, a)| a.status == AlertStatus::Published && !a.ended_by(now))
            .map(|(id, a)| (id.clone(), ServiceAlert { explains: vec![], ..a.clone() }))
            .collect();
        prop_assert_eq!(read.alerts.alerts, want);
    }
}

/// A feed as another app writes it: an empty language is no language,
/// the deprecated active period stands in for the communication period,
/// the agency names the whole agency, a direction maps to the route's
/// model direction, and a trip instance keeps its date and start. What
/// cannot be read as written is named: an entity (a direction with no
/// route), or a whole alert (no entity left, or no header).
#[test]
fn a_feed_another_app_wrote_reads_into_the_document() {
    use transit_rt::gtfs_rt::{
        alert, translated_string::Translation, Alert, EntitySelector, FeedEntity, FeedHeader, FeedMessage, TimeRange, TranslatedString,
        TripDescriptor,
    };
    let (net, _) = network();
    let text = |s: &str, lang: &str| TranslatedString { translation: vec![Translation { text: s.into(), language: Some(lang.into()) }] };
    #[allow(deprecated)]
    let entity = |id: &str, informed: Vec<EntitySelector>, header: Option<TranslatedString>| FeedEntity {
        id: id.into(),
        alert: Some(Alert {
            active_period: vec![TimeRange { start: Some(100), end: None }],
            informed_entity: informed,
            effect: Some(alert::Effect::NoService as i32),
            header_text: header,
            ..Default::default()
        }),
        ..Default::default()
    };
    let feed = FeedMessage {
        header: FeedHeader { gtfs_realtime_version: "2.0".into(), ..Default::default() },
        entity: vec![
            entity(
                "a1",
                vec![
                    EntitySelector { agency_id: Some("1".into()), ..Default::default() },
                    EntitySelector { route_id: Some("40".into()), direction_id: Some(1), ..Default::default() },
                    EntitySelector { direction_id: Some(0), ..Default::default() },
                    EntitySelector {
                        trip: Some(TripDescriptor { trip_id: Some("t10".into()), start_date: Some("20261119".into()), start_time: Some("08:00:00".into()), ..Default::default() }),
                        ..Default::default()
                    },
                ],
                Some(text("Closed", "")),
            ),
            entity("no-entity", vec![EntitySelector { direction_id: Some(0), ..Default::default() }], Some(text("x", "en"))),
            entity("no-header", vec![EntitySelector { route_id: Some("10".into()), ..Default::default() }], None),
        ],
    };
    let read = transit_rt::read_alerts(&net, &feed.encode_to_vec()).unwrap();
    let a = &read.alerts.alerts["a1"];
    assert_eq!(a.header, vec![AlertText { lang: None, text: "Closed".into() }]);
    assert_eq!(a.communication, vec![AlertPeriod { start: Some(100), end: None }]);
    assert_eq!(a.effect, AlertEffect::NoService);
    assert_eq!(
        a.informed,
        vec![
            AlertEntity::default(),
            AlertEntity { route: Some("40".into()), direction: Some("1".into()), ..Default::default() },
            AlertEntity { trip: Some("t10".into()), date: Some("2026-11-19".into()), start_secs: Some(28_800), ..Default::default() },
        ]
    );
    assert_eq!(read.skipped.len(), 2);
    assert_eq!((read.skipped[0].alert.as_str(), read.skipped[0].entity), ("a1", 2));
    let dropped: Vec<&str> = read.dropped.iter().map(|d| d.alert.as_str()).collect();
    assert_eq!(dropped, ["no-entity", "no-header"]);
    assert!(read.alerts.clone().checked().is_ok());
    assert!(transit_rt::read_alerts(&net, b"not a feed").is_err());
}

/// Wider than `arb_entity`: fields joined as an author may join them,
/// including a station, a route type, and a direction with no route.
fn arb_any_entity() -> impl Strategy<Value = AlertEntity> {
    (
        proptest::option::of(prop::sample::select(vec!["10", "20", "40", "nope"])),
        proptest::option::of(prop::sample::select(vec![3u16, 2])),
        proptest::option::of(prop::sample::select(vec!["0", "1"])),
        proptest::option::of(prop::sample::select(vec!["A", "B", "SA", "STN", "nope"])),
        proptest::option::of(prop::sample::select(vec!["t10", "t40", "nope"])),
    )
        .prop_map(|(route, route_type, direction, stop, trip)| AlertEntity {
            route: route.map(String::from),
            route_type,
            direction: direction.map(String::from),
            stop: stop.map(String::from),
            trip: trip.map(String::from),
            ..Default::default()
        })
}

proptest! {
    /// One entity read as the feed reads it: what it is published as,
    /// and the scopes it resolves to, are those of the whole round trip
    /// (encode a published alert holding only it, decode, build the
    /// live document). An entity the encoder leaves out is named, and
    /// selects nothing.
    #[test]
    fn an_entity_reads_as_the_feed_reads_it(e in arb_any_entity()) {
        let (net, boundary) = network();
        let reading = transit_rt::entity_reading(&net, &e);
        let alert = ServiceAlert {
            status: AlertStatus::Published,
            header: vec![AlertText { lang: None, text: "x".into() }],
            informed: vec![e.clone()],
            ..Default::default()
        };
        let doc = ServiceAlerts::default().put("a", alert);
        let encoded = encode_alerts(&net, &doc, 1000);
        let overview = decode_overview(&encoded.bytes).unwrap();
        let published = overview.service_alerts.first().and_then(|a| a.informed.first()).cloned();
        prop_assert_eq!(reading.published.as_ref().ok().map(|v| serde_json::to_value(v).unwrap()), published.map(|v| serde_json::to_value(v).unwrap()));
        prop_assert_eq!(reading.published.is_err(), !encoded.skipped.is_empty());
        let live = live_of(&net, &boundary, &encoded.bytes);
        let scopes = live.alerts.first().map(|a| a.scopes.clone()).unwrap_or_default();
        prop_assert_eq!(reading.scopes, scopes);
    }
}

/// The spec's own example: route and stop in one entity select that
/// route at that stop only.
#[test]
fn route_and_stop_in_one_entity_select_one_scope() {
    let (net, _) = network();
    let reading = transit_rt::entity_reading(&net, &AlertEntity { route: Some("10".into()), stop: Some("A".into()), ..Default::default() });
    assert_eq!(reading.scopes, vec![AlertScope { route: Some("10".into()), stop: Some("A".into()), ..Default::default() }]);
}

/// What an entity names, in the network's words: the route's public
/// name, the stop's name and whether it is a station (with its bays),
/// a trip's start and headsign.
#[test]
fn an_entity_names_what_it_selects() {
    let (net, _) = network();
    let named = |e: AlertEntity| transit_rt::entity_reading(&net, &e).names;
    let n = named(AlertEntity { route: Some("10".into()), stop: Some("A".into()), ..Default::default() });
    assert_eq!(n.route.as_ref().map(|r| r.name.as_str()), Some("10"));
    assert_eq!(n.stop.as_ref().map(|s| (s.name.as_str(), s.bays)), Some(("Main & 1st", 0)));
    assert!(!n.agency);
    let station = named(AlertEntity { stop: Some("STN".into()), ..Default::default() });
    assert_eq!(station.stop.as_ref().map(|s| (s.name.as_str(), s.bays)), Some(("Main Station", 2)));
    let trip = named(AlertEntity { trip: Some("t10".into()), ..Default::default() });
    let t = trip.trip.unwrap();
    assert_eq!((t.start_secs, t.headsign.as_deref(), t.route.as_deref()), (Some(28800), Some("Downtown"), Some("10")));
    assert!(named(AlertEntity::default()).agency);
    // A name the network does not hold stays its id, marked unknown.
    let gone = named(AlertEntity { stop: Some("nope".into()), ..Default::default() });
    assert_eq!(gone.stop.map(|s| (s.name, s.known)), Some(("nope".to_string(), false)));
}
