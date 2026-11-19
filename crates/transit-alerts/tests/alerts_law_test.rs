//! The service alerts document and the alert templates document: what
//! a write must pass, what a person is told about a record that passes
//! (D-152), ending an alert, the agency's list, the gap list, and the
//! template fill (D-206..D-210).
//!
//! Over the e2e fixture network: routes 10, 20, 40 (route 40 runs
//! directions 0 and 1), stops A ("Main & 1st"), B, C, and the station
//! STN with bays SA and SB; trip t10 at 08:00.

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_alerts::*;
use transit_live::{AlertEffect, AlertPeriod};

fn network() -> transit_network::Network {
    let feed = transit_codec::Feed::from_zip_bytes(include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    transit_codec::import(&feed).network
}

fn text(s: &str) -> Vec<AlertText> {
    vec![AlertText { lang: None, text: s.into() }]
}

fn period(start: Option<u64>, end: Option<u64>) -> AlertPeriod {
    AlertPeriod { start, end }
}

fn route(r: &str) -> AlertEntity {
    AlertEntity { route: Some(r.into()), ..Default::default() }
}

fn stop(s: &str) -> AlertEntity {
    AlertEntity { stop: Some(s.into()), ..Default::default() }
}

fn alert(informed: Vec<AlertEntity>, communication: Vec<AlertPeriod>) -> ServiceAlert {
    ServiceAlert {
        status: AlertStatus::Published,
        effect: AlertEffect::Detour,
        header: text("Route 10 detour"),
        communication,
        informed,
        ..Default::default()
    }
}

fn doc(alerts: Vec<(&str, ServiceAlert)>) -> ServiceAlerts {
    let mut d = ServiceAlerts::default();
    for (id, a) in alerts {
        d = d.put(id, a);
    }
    d
}

fn problems(d: ServiceAlerts) -> Vec<AlertProblem> {
    d.checked().err().unwrap_or_default().into_iter().map(|r| r.problem).collect()
}

// ---------------------------------------------------------------------
// What a write must pass: only what makes a record wrong.
// ---------------------------------------------------------------------

#[test]
fn a_well_formed_alert_passes() {
    assert!(doc(vec![("a1", alert(vec![route("10")], vec![]))]).checked().is_ok());
}

#[test]
fn a_record_is_refused_only_for_what_makes_it_wrong() {
    let cases: Vec<(ServiceAlert, AlertProblem)> = vec![
        (alert(vec![], vec![]), AlertProblem::NoEntity),
        (ServiceAlert { header: vec![], ..alert(vec![route("10")], vec![]) }, AlertProblem::NoHeader),
        (ServiceAlert { header: text("  "), ..alert(vec![route("10")], vec![]) }, AlertProblem::NoHeader),
        (alert(vec![route("10")], vec![period(Some(200), Some(100))]), AlertProblem::EndBeforeStart),
        (
            ServiceAlert { impact: vec![period(Some(50), Some(150))], ..alert(vec![route("10")], vec![period(Some(100), Some(200))]) },
            AlertProblem::ImpactOutsideCommunication,
        ),
        (
            alert(vec![AlertEntity { direction: Some("0".into()), ..Default::default() }], vec![]),
            AlertProblem::DirectionWithoutRoute,
        ),
        (
            alert(vec![AlertEntity { start_secs: Some(28_800), ..Default::default() }], vec![]),
            AlertProblem::InstanceWithoutTrip,
        ),
        (
            alert(vec![AlertEntity { trip: Some("t10".into()), date: Some("2026-13-01".into()), ..Default::default() }], vec![]),
            AlertProblem::BadDate { date: "2026-13-01".into() },
        ),
        (
            ServiceAlert {
                header: vec![AlertText { lang: Some("es".into()), text: "a".into() }, AlertText { lang: Some("es".into()), text: "b".into() }],
                ..alert(vec![route("10")], vec![])
            },
            AlertProblem::RepeatedLanguage { field: "header".into(), lang: Some("es".into()) },
        ),
    ];
    for (a, want) in cases {
        assert_eq!(problems(doc(vec![("a1", a)])), vec![want.clone()], "{want:?}");
    }
    assert_eq!(problems(doc(vec![("", alert(vec![route("10")], vec![]))])), vec![AlertProblem::NoId]);
}

#[test]
fn an_impact_period_inside_a_communication_period_passes() {
    let a = ServiceAlert {
        impact: vec![period(Some(120), Some(180))],
        ..alert(vec![route("10")], vec![period(Some(100), Some(200))])
    };
    assert!(doc(vec![("a1", a)]).checked().is_ok());
    // An open communication period holds any impact after its start.
    let open = ServiceAlert { impact: vec![period(Some(120), None)], ..alert(vec![route("10")], vec![period(Some(100), None)]) };
    assert!(doc(vec![("a1", open)]).checked().is_ok());
}

// ---------------------------------------------------------------------
// What a person is told: a record that passes, with consequences.
// ---------------------------------------------------------------------

#[test]
fn an_id_the_network_does_not_have_is_flagged_not_refused() {
    let net = network();
    let a = alert(
        vec![
            route("99"),
            stop("Z"),
            AlertEntity { trip: Some("nope".into()), ..Default::default() },
            AlertEntity { route: Some("10".into()), direction: Some("1".into()), ..Default::default() },
        ],
        vec![],
    );
    let d = doc(vec![("a1", a)]);
    assert!(d.clone().checked().is_ok());
    let flags: Vec<AlertNotice> = d.flags(&net, &[]).into_iter().map(|f| f.notice).collect();
    assert_eq!(
        flags,
        vec![
            AlertNotice::UnknownRoute { route: "99".into() },
            AlertNotice::UnknownStop { stop: "Z".into() },
            AlertNotice::UnknownTrip { trip: "nope".into() },
            AlertNotice::UnknownDirection { route: "10".into(), direction: "1".into() },
        ]
    );
    assert!(doc(vec![("a1", alert(vec![route("40"), stop("SA")], vec![]))]).flags(&net, &[]).is_empty());
}

#[test]
fn a_missing_translation_is_flagged() {
    let net = network();
    let a = ServiceAlert {
        header: vec![AlertText { lang: Some("en".into()), text: "Detour".into() }, AlertText { lang: Some("es".into()), text: "Desvío".into() }],
        description: vec![AlertText { lang: Some("en".into()), text: "Stop C is closed.".into() }],
        ..alert(vec![route("10")], vec![])
    };
    let flags: Vec<AlertNotice> = doc(vec![("a1", a)]).flags(&net, &["en".into(), "es".into(), "ru".into()]).into_iter().map(|f| f.notice).collect();
    assert_eq!(
        flags,
        vec![
            AlertNotice::MissingTranslation { field: "header".into(), lang: "ru".into() },
            AlertNotice::MissingTranslation { field: "description".into(), lang: "es".into() },
            AlertNotice::MissingTranslation { field: "description".into(), lang: "ru".into() },
        ]
    );
}

// ---------------------------------------------------------------------
// Ending an alert, and the agency's list.
// ---------------------------------------------------------------------

fn arb_period() -> impl Strategy<Value = AlertPeriod> {
    (proptest::option::of(0u64..1000), proptest::option::of(0u64..1000)).prop_map(|(a, b)| match (a, b) {
        (Some(a), Some(b)) => period(Some(a.min(b)), Some(a.max(b))),
        (a, b) => period(a, b),
    })
}

proptest! {
    /// Ending at `at` changes nothing up to `at`, and nothing after it
    /// is in force. The record stays, with its id.
    #[test]
    fn ending_keeps_the_past_and_closes_the_future(periods in prop::collection::vec(arb_period(), 0..4), at in 0u64..1000) {
        let d = doc(vec![("a1", alert(vec![route("10")], periods))]);
        let ended = d.end("a1", at).unwrap();
        prop_assert!(ended.clone().checked().is_ok());
        let (before, after) = (&d.alerts["a1"], &ended.alerts["a1"]);
        for t in 0..1100u64 {
            if t <= at {
                prop_assert_eq!(after.in_force(t), before.in_force(t), "t {}", t);
            } else {
                prop_assert!(!after.in_force(t), "t {} after the end", t);
            }
        }
    }
}

#[test]
fn ending_an_unknown_alert_is_none() {
    assert!(ServiceAlerts::default().end("a1", 10).is_none());
}

#[test]
fn only_a_published_alert_is_in_force() {
    let mut a = alert(vec![route("10")], vec![]);
    assert!(a.in_force(5));
    a.status = AlertStatus::Draft;
    assert!(!a.in_force(5));
    a.status = AlertStatus::Retired;
    assert!(!a.in_force(5));
}

/// The agency's list: every alert in exactly one group. In force now,
/// upcoming, ended (or withdrawn), and drafts; each in force alert with
/// no end says how long it has been open.
#[test]
fn the_list_puts_every_alert_in_one_group() {
    let now = 10_000;
    let d = doc(vec![
        ("always", alert(vec![route("10")], vec![])),
        ("open", alert(vec![route("10")], vec![period(Some(now - 600), None)])),
        ("closing", alert(vec![route("10")], vec![period(Some(now - 600), Some(now + 600))])),
        ("later", alert(vec![route("10")], vec![period(Some(now + 600), None)])),
        ("past", alert(vec![route("10")], vec![period(Some(now - 900), Some(now - 600))])),
        ("withdrawn", ServiceAlert { status: AlertStatus::Retired, ..alert(vec![route("10")], vec![]) }),
        ("draft", ServiceAlert { status: AlertStatus::Draft, ..alert(vec![route("10")], vec![]) }),
    ]);
    let list = d.list(now);
    let ids = |rows: &[AlertRow]| rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&list.in_force), ["always", "closing", "open"]);
    assert_eq!(ids(&list.upcoming), ["later"]);
    assert_eq!(ids(&list.ended), ["past", "withdrawn"]);
    assert_eq!(ids(&list.drafts), ["draft"]);
    let open = |id: &str| list.in_force.iter().find(|r| r.id == id).unwrap().open_secs;
    assert_eq!(open("open"), Some(600));
    assert_eq!(open("closing"), None, "it has an end");
    assert_eq!(open("always"), None, "no period: no start to count from");
}

// ---------------------------------------------------------------------
// The gap list: what is in force that no alert explains.
// ---------------------------------------------------------------------

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 19).unwrap()
}

fn edits() -> Vec<transit_sync::EditRecord> {
    let diversion: transit_sync::Diversion = serde_json::from_value(serde_json::json!({
        "id": "d1", "route_id": "10", "first_kept_stop": "A", "last_kept_stop": "C",
        "skipped_stops": ["B"], "start_date": "2026-11-18", "end_date": "2026-11-20"
    }))
    .unwrap();
    let cancel: transit_sync::TripCancel =
        serde_json::from_value(serde_json::json!({ "id": "c1", "trip_id": "t10", "date": "2026-11-19" })).unwrap();
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
    serde_json::from_value(serde_json::json!({
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

fn gap_sources(alerts: &ServiceAlerts) -> Vec<AlertSource> {
    gaps(alerts, &edits(), &stops(), day(), 10_000).into_iter().map(|g| g.source).collect()
}

#[test]
fn the_gap_list_names_what_no_alert_explains() {
    let all = vec![AlertSource::Edit { id: "r-div".into() }, AlertSource::Edit { id: "r-cancel".into() }, AlertSource::Stop { stop: "B".into() }];
    // No alert: every submitted or accepted edit in force, and every
    // suspended stop. A draft edit is not yet anyone's news, and C's
    // suspension has ended.
    assert_eq!(gap_sources(&ServiceAlerts::default()), all);
    // An alert that explains one takes it off the list.
    let explaining = |sources: Vec<AlertSource>, status: AlertStatus, communication: Vec<AlertPeriod>| ServiceAlert {
        explains: sources,
        status,
        ..alert(vec![route("10")], communication)
    };
    let d = doc(vec![("a1", explaining(vec![AlertSource::Edit { id: "r-div".into() }, AlertSource::Stop { stop: "B".into() }], AlertStatus::Published, vec![]))]);
    assert_eq!(gap_sources(&d), vec![AlertSource::Edit { id: "r-cancel".into() }]);
    // A draft explains too: someone is writing it.
    let d = doc(vec![("a1", explaining(vec![AlertSource::Edit { id: "r-cancel".into() }], AlertStatus::Draft, vec![]))]);
    assert!(!gap_sources(&d).contains(&AlertSource::Edit { id: "r-cancel".into() }));
    // A withdrawn alert, or one that has ended, explains nothing now.
    let d = doc(vec![
        ("gone", explaining(vec![AlertSource::Edit { id: "r-div".into() }], AlertStatus::Retired, vec![])),
        ("over", explaining(vec![AlertSource::Stop { stop: "B".into() }], AlertStatus::Published, vec![period(Some(0), Some(5_000))])),
    ]);
    assert_eq!(gap_sources(&d), all);
}

#[test]
fn a_gap_carries_what_an_alert_about_it_starts_from() {
    let g = gaps(&ServiceAlerts::default(), &edits(), &stops(), day(), 10_000);
    let div = &g[0];
    assert_eq!((div.route.as_deref(), div.stops.clone()), (Some("10"), vec!["B".to_string()]));
    assert_eq!((div.start_date.as_deref(), div.end_date.as_deref()), (Some("2026-11-18"), Some("2026-11-20")));
    assert_eq!(div.effect, AlertEffect::Detour);
    let cancel = &g[1];
    assert_eq!((cancel.trip.as_deref(), cancel.effect), (Some("t10"), AlertEffect::NoService));
    let stop_b = &g[2];
    assert_eq!((stop_b.stops.clone(), stop_b.start_date.as_deref(), stop_b.end_date.as_deref()), (vec!["B".to_string()], Some("2026-11-01"), Some("2026-12-01")));
    assert_eq!(stop_b.effect, AlertEffect::NoService);
}

// ---------------------------------------------------------------------
// Templates: the agency's standard language, with blanks.
// ---------------------------------------------------------------------

fn template(header: &str) -> AlertTemplate {
    AlertTemplate { name: "Stop closure".into(), effect: AlertEffect::NoService, header: text(header), ..Default::default() }
}

fn templates(t: AlertTemplate) -> AlertTemplates {
    AlertTemplates::default().put("closure", t)
}

#[test]
fn a_template_passes_or_names_its_problem() {
    assert!(templates(template("The stop at {stop} is closed starting {start date}")).checked().is_ok());
    let problems = |t: AlertTemplate| templates(t).checked().err().unwrap_or_default().into_iter().map(|r| r.problem).collect::<Vec<_>>();
    assert_eq!(problems(AlertTemplate { name: " ".into(), ..template("x") }), vec![TemplateProblem::NoName]);
    assert_eq!(problems(template("")), vec![TemplateProblem::NoHeader]);
    assert_eq!(problems(template("Closed at {stop name}")), vec![TemplateProblem::UnknownBlank { blank: "stop name".into() }]);
}

#[test]
fn the_fill_writes_names_and_dates_in_the_agencys_words() {
    let net = network();
    let t = AlertTemplate {
        description: text("Use {route} at the stops near {stop} until {end date}."),
        ..template("The stop at {stop} is closed starting {start date}")
    };
    let input = FillInput {
        stops: vec!["A".into()],
        routes: vec!["10".into(), "20".into(), "40".into()],
        start_date: Some("2026-08-17".into()),
        end_date: Some("2026-11-20".into()),
    };
    let f = fill(&t, &net, &input);
    assert_eq!(f.header[0].text, "The stop at Main & 1st is closed starting Monday, August 17");
    assert_eq!(f.description[0].text, "Use 10, 20, and 40 at the stops near Main & 1st until Friday, November 20.");
    assert!(f.unfilled.is_empty());
    assert_eq!(f.effect, AlertEffect::NoService);
    let two = fill(&template("{stop}"), &net, &FillInput { stops: vec!["A".into(), "B".into()], ..Default::default() });
    assert_eq!(two.header[0].text, "Main & 1st and Main & 2nd");
}

proptest! {
    /// Every blank is filled, or the fill names it; no blank is left in
    /// a text that the fill does not name.
    #[test]
    fn every_blank_is_filled_or_named(
        stops in prop::collection::vec(prop::sample::select(vec!["A", "B", "SA", "Z"]), 0..3),
        start in proptest::option::of(prop::sample::select(vec!["2026-08-17", "bad"])),
        lang in proptest::option::of(prop::sample::select(vec!["en", "es"])),
    ) {
        let net = network();
        let t = AlertTemplate {
            header: vec![AlertText { lang: lang.map(String::from), text: "At {stop} on {start date}, {route}; {end date}".into() }],
            ..template("x")
        };
        let input = FillInput { stops: stops.iter().map(|s| s.to_string()).collect(), start_date: start.map(String::from), ..Default::default() };
        let f = fill(&t, &net, &input);
        let out = &f.header[0].text;
        for blank in BLANKS {
            let left = out.contains(&format!("{{{blank}}}"));
            let named = f.unfilled.iter().any(|u| u.blank == *blank && u.field == "header");
            prop_assert_eq!(left, named, "blank {} in {:?}", blank, out);
        }
    }
}
