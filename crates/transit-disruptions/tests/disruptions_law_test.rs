//! What is disrupted on a day, and whether an alert explains it
//! (D-213, D-218): the disruptions from the edit document, the live
//! feed, and the stops document; the gap list; and the day by route.
//!
//! The laws come from the user's intent (2026-11-19): the live feed's
//! cancels are shown as the feed gives them, beside the edit document's,
//! and no rule picks a winner; a rider's question about a route or a
//! stop is answered from one list; the notes are read beside it.
//!
//! Over the e2e fixture network on Thursday 2026-11-19 (the WK service
//! runs): route 10 runs t10 (A → B at 08:00) and t30 (A → C → B at
//! 08:40); route 20 runs t20 (A → B at 08:20); route 40 runs t40 and
//! t41 between the station's bays SA and SB. Stop A is "Main & 1st".

use chrono::NaiveDate;
use serde_json::json;
use transit_alerts::{AlertEffect, AlertPeriod, AlertSource, AlertStatus, ServiceAlert, ServiceAlerts};
use transit_disruptions::*;
use transit_notes::{NoteTarget, Notes};

fn network() -> transit_network::Network {
    let feed = transit_codec::Feed::from_zip_bytes(include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    transit_codec::import(&feed).network
}

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 19).unwrap()
}

const NOW: u64 = 10_000;

fn edits() -> Vec<transit_sync::EditRecord> {
    let diversion: transit_sync::Diversion = serde_json::from_value(json!({
        "id": "d1", "route_id": "10", "first_kept_stop": "A", "last_kept_stop": "C",
        "skipped_stops": ["B"], "start_date": "2026-11-18", "end_date": "2026-11-20"
    }))
    .unwrap();
    let cancel: transit_sync::TripCancel =
        serde_json::from_value(json!({ "id": "c1", "trip_id": "t10", "start_secs": 28800, "date": "2026-11-19" })).unwrap();
    let record = |id: &str, status: transit_sync::EditStatus, d: Option<transit_sync::Diversion>, c: Option<transit_sync::TripCancel>| {
        transit_sync::EditRecord { id: id.into(), diversion: d, cancel: c, status, rev: 1, updated_at: 0, extra: Default::default() }
    };
    use transit_sync::EditStatus::*;
    vec![
        record("r-div", Accepted, Some(diversion.clone()), None),
        record("r-cancel", Submitted, None, Some(cancel)),
        record("r-draft", Draft, Some(diversion.clone()), None),
        record("r-retired", Retired, Some(diversion), None),
    ]
}

fn stops() -> transit_stops::Stops {
    serde_json::from_value(json!({
        "stops": {
            "B": [
                { "id": "e0", "event": "in_service", "lat": 45.51, "lon": -73.51 },
                { "id": "e1", "on": "2026-11-01", "event": "suspended", "until": "2026-12-01" }
            ],
            "C": [
                { "id": "e0", "event": "in_service", "lat": 45.52, "lon": -73.52 },
                { "id": "e1", "on": "2026-11-01", "event": "suspended", "until": "2026-11-10" }
            ]
        }
    }))
    .unwrap()
}

/// The live feed: t10's 08:00 instance cancelled (the edit document
/// cancels it too), t20 running 6 minutes late, and t41 2 minutes late.
fn live() -> transit_live::LiveDay {
    serde_json::from_value(json!({
        "kind": "live", "schema_version": 2,
        "window": { "early_secs": 0, "late_secs": 60, "very_late_secs": 300 },
        "trips": [
            { "trip": "t10", "date": "2026-11-19", "start_secs": 28800, "route": "10", "status": "no_prediction", "canceled": true, "stops": [] },
            { "trip": "t20", "date": "2026-11-19", "start_secs": 30000, "route": "20", "worst_delay_secs": 360, "status": "very_late", "stops": [] },
            { "trip": "t41", "date": "2026-11-19", "start_secs": 30900, "route": "40", "worst_delay_secs": 120, "status": "late", "stops": [] }
        ],
        "vehicles": [], "unmatched_trips": 0
    }))
    .unwrap()
}

fn feed_cancel() -> AlertSource {
    AlertSource::Trip { trip: "t10".into(), date: "2026-11-19".into(), start_secs: 28800 }
}

fn explaining(sources: Vec<AlertSource>, status: AlertStatus, communication: Vec<AlertPeriod>) -> ServiceAlert {
    serde_json::from_value::<ServiceAlert>(json!({ "status": "published", "header": [{ "text": "x" }], "informed": [{}] }))
        .map(|a| ServiceAlert { explains: sources, status, communication, ..a })
        .unwrap()
}

fn alerts(list: Vec<(&str, ServiceAlert)>) -> ServiceAlerts {
    list.into_iter().fold(ServiceAlerts::default(), |d, (id, a)| d.put(id, a))
}

struct World {
    network: transit_network::Network,
    edits: Vec<transit_sync::EditRecord>,
    live: Option<transit_live::LiveDay>,
    stops: transit_stops::Stops,
    alerts: ServiceAlerts,
    notes: Notes,
}

fn world() -> World {
    World { network: network(), edits: edits(), live: Some(live()), stops: stops(), alerts: ServiceAlerts::default(), notes: Notes::default() }
}

impl World {
    fn inputs(&self) -> Inputs<'_> {
        Inputs {
            network: &self.network,
            edits: &self.edits,
            live: self.live.as_ref(),
            stops: &self.stops,
            alerts: &self.alerts,
            notes: &self.notes,
            date: day(),
            now: NOW,
        }
    }
}

fn sources(list: &[Disruption]) -> Vec<AlertSource> {
    list.iter().map(|d| d.source.clone()).collect()
}

// ---------------------------------------------------------------------
// The disruptions: each source as its document gives it.
// ---------------------------------------------------------------------

#[test]
fn every_source_is_listed_as_its_document_gives_it() {
    let w = world();
    let all = disruptions(&w.inputs());
    // The edit document's in force (a draft is not yet anyone's news),
    // in its order; then the feed's cancels, by start; then each stop
    // suspended on the day, by id. C's suspension has ended.
    assert_eq!(
        sources(&all),
        vec![AlertSource::Edit { id: "r-div".into() }, AlertSource::Edit { id: "r-cancel".into() }, feed_cancel(), AlertSource::Stop { stop: "B".into() }]
    );
    let from: Vec<Origin> = all.iter().map(|d| d.from).collect();
    assert_eq!(from, vec![Origin::Edits, Origin::Edits, Origin::Feed, Origin::Stops]);
}

#[test]
fn a_cancel_in_both_sources_is_listed_twice_and_no_rule_picks_one() {
    let w = world();
    let all = disruptions(&w.inputs());
    let cancels: Vec<&Disruption> = all.iter().filter(|d| d.trip.as_deref() == Some("t10")).collect();
    assert_eq!(cancels.len(), 2);
    assert_eq!((cancels[0].from, cancels[1].from), (Origin::Edits, Origin::Feed));
}

#[test]
fn a_cancel_carries_the_names_a_rider_uses_and_the_next_trip() {
    let w = world();
    let all = disruptions(&w.inputs());
    let c = all.iter().find(|d| d.source == feed_cancel()).unwrap();
    assert_eq!(c.route.as_deref(), Some("10"));
    assert_eq!(c.effect, AlertEffect::NoService);
    assert_eq!(c.start_secs, Some(28800));
    let a = &w.network.stops["A"];
    assert_eq!(c.first_stop, Some(NamedStop { id: "A".into(), name: Some("Main & 1st".into()), lat: Some(a.lat), lon: Some(a.lon) }));
    // The next trip of the route from the same first stop in the same
    // direction that runs: t30 at 08:40.
    assert_eq!(c.next_start_secs, Some(31200));
}

#[test]
fn a_detour_and_a_closed_stop_name_their_stops() {
    let w = world();
    let all = disruptions(&w.inputs());
    let detour = &all[0];
    assert_eq!(detour.effect, AlertEffect::Detour);
    assert_eq!(detour.stops.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["B"]);
    assert_eq!((detour.start_date.as_deref(), detour.end_date.as_deref()), (Some("2026-11-18"), Some("2026-11-20")));
    let closed = all.iter().find(|d| d.from == Origin::Stops).unwrap();
    assert_eq!(closed.stops[0].name.as_deref(), Some("Main & 2nd"));
    // A closed stop is where the stops document puts it on the day.
    assert_eq!((closed.stops[0].lat, closed.stops[0].lon), (Some(45.51), Some(-73.51)));
    assert_eq!((closed.start_date.as_deref(), closed.end_date.as_deref()), (Some("2026-11-01"), Some("2026-12-01")));
}

#[test]
fn no_live_document_lists_no_feed_cancels() {
    let mut w = world();
    w.live = None;
    assert!(!sources(&disruptions(&w.inputs())).contains(&feed_cancel()));
}

// ---------------------------------------------------------------------
// What explains it, and what was noted about it.
// ---------------------------------------------------------------------

#[test]
fn an_alert_explains_what_it_names_while_it_is_not_withdrawn_or_ended() {
    let mut w = world();
    w.alerts = alerts(vec![
        ("a-feed", explaining(vec![feed_cancel()], AlertStatus::Published, vec![])),
        ("a-draft", explaining(vec![AlertSource::Edit { id: "r-cancel".into() }], AlertStatus::Draft, vec![])),
        ("a-gone", explaining(vec![AlertSource::Edit { id: "r-div".into() }], AlertStatus::Retired, vec![])),
        ("a-over", explaining(vec![AlertSource::Stop { stop: "B".into() }], AlertStatus::Published, vec![AlertPeriod { start: Some(0), end: Some(5_000) }])),
    ]);
    let all = disruptions(&w.inputs());
    let by = |s: &AlertSource| all.iter().find(|d| &d.source == s).unwrap().explained_by.clone();
    assert_eq!(by(&feed_cancel()), vec!["a-feed"]);
    assert_eq!(by(&AlertSource::Edit { id: "r-cancel".into() }), vec!["a-draft"], "a draft explains: someone is writing it");
    assert!(by(&AlertSource::Edit { id: "r-div".into() }).is_empty(), "a withdrawn alert explains nothing");
    assert!(by(&AlertSource::Stop { stop: "B".into() }).is_empty(), "an ended alert explains nothing now");
    // The gap list is exactly what no alert explains.
    assert_eq!(gaps(&w.inputs()).into_iter().map(|g| g.source).collect::<Vec<_>>(), vec![AlertSource::Edit { id: "r-div".into() }, AlertSource::Stop { stop: "B".into() }]);
}

#[test]
fn a_gap_carries_what_an_alert_about_it_starts_from() {
    let w = world();
    let g = gaps(&w.inputs());
    let feed = g.iter().find(|g| g.source == feed_cancel()).unwrap();
    assert_eq!((feed.trip.as_deref(), feed.start_secs, feed.effect), (Some("t10"), Some(28800), AlertEffect::NoService));
    assert_eq!((feed.start_date.as_deref(), feed.end_date.as_deref()), (Some("2026-11-19"), Some("2026-11-19")));
    let stop_b = g.iter().find(|g| g.source == AlertSource::Stop { stop: "B".into() }).unwrap();
    assert_eq!(stop_b.stops, vec!["B".to_string()]);
}

#[test]
fn a_gap_names_the_riders_an_alert_about_it_informs() {
    let w = world();
    let g = gaps(&w.inputs());
    let entity = |v: serde_json::Value| -> transit_alerts::AlertEntity { serde_json::from_value(v).unwrap() };
    // A cancelled trip: its instance, on its route.
    let feed = g.iter().find(|g| g.source == feed_cancel()).unwrap();
    assert_eq!(feed.informed, vec![entity(json!({ "route": "10", "trip": "t10", "date": "2026-11-19", "start_secs": 28800 }))]);
    // A detour: its route, and its route at each stop it skips.
    let detour = g.iter().find(|g| g.effect == AlertEffect::Detour).unwrap();
    assert_eq!(detour.informed, vec![entity(json!({ "route": "10" })), entity(json!({ "route": "10", "stop": "B" }))]);
    // A closed stop: the stop.
    let stop_b = g.iter().find(|g| g.source == AlertSource::Stop { stop: "B".into() }).unwrap();
    assert_eq!(stop_b.informed, vec![entity(json!({ "stop": "B" }))]);
}

#[test]
fn each_disruption_carries_its_latest_note() {
    let mut w = world();
    let on_trip = NoteTarget::Trip { trip: "t10".into(), date: "2026-11-19".into(), start_secs: 28800 };
    w.notes = Notes::default()
        .write("n1", on_trip.clone(), "No operator.", "dispatch", 100)
        .unwrap()
        .write("n2", on_trip, "Two calls from riders.", "service", 200)
        .unwrap()
        .write("n3", NoteTarget::Edit { id: "r-div".into() }, "Police on scene.", "dispatch", 150)
        .unwrap();
    let all = disruptions(&w.inputs());
    let note = |s: &AlertSource| all.iter().find(|d| &d.source == s).unwrap().latest_note.as_ref().map(|r| r.id.clone());
    assert_eq!(note(&feed_cancel()), Some("n2".into()));
    assert_eq!(note(&AlertSource::Edit { id: "r-div".into() }), Some("n3".into()));
    assert_eq!(note(&AlertSource::Stop { stop: "B".into() }), None);
}

// ---------------------------------------------------------------------
// The day by route.
// ---------------------------------------------------------------------

#[test]
fn the_day_lists_each_route_with_a_disruption_or_a_late_trip() {
    let w = world();
    let t = today(&w.inputs(), &Filter::default());
    let routes: Vec<&str> = t.routes.iter().map(|r| r.route.as_str()).collect();
    // Route 10: its detour, both cancels, and the closed stop B, which
    // it serves. Route 20 serves B too, and runs late. Route 40 runs
    // late. Each route is listed once.
    assert_eq!(routes, ["10", "20", "40"]);
    assert_eq!(t.unaffected_routes, (w.network.routes.len() - 3) as u32);
    let r10 = &t.routes[0];
    assert_eq!(sources(&r10.disruptions).len(), 4);
    let r20 = &t.routes[1];
    assert_eq!(sources(&r20.disruptions), vec![AlertSource::Stop { stop: "B".into() }]);
    assert_eq!((r20.late_trips, r20.worst_delay_secs), (1, Some(360)));
    assert_eq!((t.routes[2].late_trips, t.routes[2].worst_delay_secs), (1, Some(120)));
    assert_eq!((r10.late_trips, r10.worst_delay_secs), (0, None), "a cancelled trip is not late");
}

#[test]
fn a_filter_answers_one_route_or_one_stop() {
    let w = world();
    let one = today(&w.inputs(), &Filter { route: Some("20".into()), stop: None });
    assert_eq!(one.routes.iter().map(|r| r.route.as_str()).collect::<Vec<_>>(), ["20"]);
    // At stop C: route 10's t30 serves it, so nothing of route 10's
    // that names C or a trip through C. The detour skips B only, and
    // t10 does not serve C: route 10 has no disruption at C.
    let at_c = today(&w.inputs(), &Filter { route: None, stop: Some("C".into()) });
    assert!(at_c.routes.is_empty());
    // At stop B: the closure, on both routes that serve it; route 10's
    // detour skips it, and t10 serves it.
    let at_b = today(&w.inputs(), &Filter { route: None, stop: Some("B".into()) });
    assert_eq!(at_b.routes.iter().map(|r| r.route.as_str()).collect::<Vec<_>>(), ["10", "20"]);
    assert_eq!(at_b.routes[0].disruptions.len(), 4);
}

#[test]
fn a_station_answers_what_touches_its_bays() {
    // t40 leaves the station's bay A at 08:15. A rider asks about the
    // station, not the bay: the station answers the cancel at its bay.
    let mut w = world();
    let cancel: transit_sync::TripCancel =
        serde_json::from_value(json!({ "id": "c40", "trip_id": "t40", "start_secs": 29700, "date": "2026-11-19" })).unwrap();
    w.edits.push(transit_sync::EditRecord {
        id: "r-40".into(),
        diversion: None,
        cancel: Some(cancel),
        status: transit_sync::EditStatus::Accepted,
        rev: 1,
        updated_at: 0,
        extra: Default::default(),
    });
    let at_bay = today(&w.inputs(), &Filter { route: None, stop: Some("SA".into()) });
    let at_station = today(&w.inputs(), &Filter { route: None, stop: Some("STN".into()) });
    assert_eq!(at_station, at_bay);
    assert_eq!(at_station.routes.iter().map(|r| r.route.as_str()).collect::<Vec<_>>(), ["40"]);
}

// ---------------------------------------------------------------------
// A new alert (D-228): what it starts from is the library's answer, so
// no host decides what a new alert informs, explains, and does.
// ---------------------------------------------------------------------

#[test]
fn a_new_alert_for_a_gap_starts_from_what_the_gap_says() {
    let w = world();
    let g = gaps(&w.inputs());
    let gap = g.iter().find(|g| g.source == feed_cancel()).unwrap();
    let start = start_alert(&w.inputs(), Some(&feed_cancel()));
    let a = &start.alert;
    assert_eq!((a.status, a.cause, a.effect, a.severity), (AlertStatus::Draft, transit_alerts::AlertCause::Unknown, gap.effect, transit_alerts::AlertSeverity::Unknown));
    assert_eq!(a.informed, gap.informed);
    assert_eq!(a.explains, vec![feed_cancel()]);
    assert_eq!(a.header.len(), 1);
    assert!(a.header[0].text.is_empty(), "the header is left for the writer");
    assert_eq!(a.communication, vec![AlertPeriod { start: Some(NOW), end: None }], "told to riders from now until someone sets an end");
}

#[test]
fn the_preview_stop_is_a_stop_the_gap_takes_away_else_its_trips_first_stop() {
    let w = world();
    // a closed stop names its own stop
    let closed = start_alert(&w.inputs(), Some(&AlertSource::Stop { stop: "B".into() }));
    assert_eq!(closed.preview_stop.as_deref(), Some("B"));
    // a cancelled trip takes away no stop: its trip's first stop, as the day names it
    let all = disruptions(&w.inputs());
    let first = all.iter().find(|d| d.source == feed_cancel()).unwrap().first_stop.as_ref().map(|s| s.id.clone());
    assert!(first.is_some());
    assert_eq!(start_alert(&w.inputs(), Some(&feed_cancel())).preview_stop, first);
}

#[test]
fn a_new_alert_for_an_explained_disruption_is_blank_and_explains_it() {
    let mut w = world();
    w.alerts = alerts(vec![("a-feed", explaining(vec![feed_cancel()], AlertStatus::Published, vec![]))]);
    let start = start_alert(&w.inputs(), Some(&feed_cancel()));
    assert_eq!(start.alert.explains, vec![feed_cancel()]);
    assert!(start.alert.informed.is_empty(), "no gap: nothing to start from");
    assert_eq!(start.alert.effect, AlertEffect::Unknown);
    assert_eq!(start.preview_stop, None);
}

#[test]
fn a_blank_new_alert_explains_nothing_and_is_told_from_now() {
    let w = world();
    let start = start_alert(&w.inputs(), None);
    assert!(start.alert.explains.is_empty() && start.alert.informed.is_empty());
    assert_eq!(start.alert.communication, vec![AlertPeriod { start: Some(NOW), end: None }]);
    assert_eq!(start.preview_stop, None);
}
