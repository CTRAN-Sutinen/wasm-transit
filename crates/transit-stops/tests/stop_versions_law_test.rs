//! GTFS export and import with the stops' history. The
//! ops are in cap-stops' OPS.md (`export_versions`, `import_events`).

use transit_date::Date;
use proptest::prelude::*;
use transit_stops::stop_record::{feed_events, import_feed, AddedEvent, export_versions, import_events, read_records, seed};
use transit_network::{materialize, Network};
use transit_stops::{Details, Event, EventKind, Stops};

fn fixture() -> Network {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    let load: serde_json::Value = serde_json::from_str(&text).unwrap();
    serde_json::from_value(load["network"].clone()).unwrap()
}

/// The fixture runs its weekday and Sunday service through 2026.
fn day(n: u32) -> Date {
    Date::from_ymd_opt(2026, 9, 1).unwrap() + transit_date::Days::new(n as i64)
}

fn date(n: u32) -> String {
    day(n).to_string()
}

fn ev(kind: EventKind, on: u32) -> Event {
    Event { on: Some(date(on)), event: kind, ..Event::default() }
}

#[derive(Debug, Clone)]
enum Change {
    Move(u32, i32, Option<u32>),
    Rename(u32),
    Suspend(u32, Option<u32>),
    Retire(u32),
}

fn arb_change() -> impl Strategy<Value = Change> {
    prop_oneof![
        (0u32..60, -300i32..300, proptest::option::of(1u32..30)).prop_map(|(on, d, len)| Change::Move(on, d, len)),
        (0u32..60).prop_map(Change::Rename),
        (0u32..60, proptest::option::of(1u32..30)).prop_map(|(on, len)| Change::Suspend(on, len)),
        (0u32..60).prop_map(Change::Retire),
    ]
}

/// The fixture's stops seeded, some given up to three changes.
fn arb_stops(net: &Network) -> impl Strategy<Value = Stops> {
    let ids: Vec<String> = net.stops.keys().cloned().collect();
    let base = seed(net);
    proptest::collection::vec((0usize..ids.len(), arb_change()), 0..4).prop_map(move |changes| {
        let mut doc = base.clone();
        for (i, change) in changes {
            let id = &ids[i];
            let e = match change {
                Change::Move(on, d, len) => {
                    let s = &doc.stops[id][0];
                    Event { lat: Some(s.lat.unwrap() + d as f64 * 1e-5), lon: s.lon, until: len.map(|l| date(on + l)), ..ev(EventKind::Moved, on) }
                }
                Change::Rename(on) => Event { details: Details { name: Some(format!("Renamed {on}")), ..Details::default() }, ..ev(EventKind::Changed, on) },
                Change::Suspend(on, len) => Event { until: len.map(|l| date(on + l)), ..ev(EventKind::Suspended, on) },
                Change::Retire(on) => ev(EventKind::Retired, on),
            };
            let next = doc.add_event(id, e);
            if next.clone().checked().is_ok() {
                doc = next;
            }
        }
        doc
    })
}

/// Takes off the `@<date>` that a version or a split adds.
fn base(id: &str) -> String {
    match id.rsplit_once('@') {
        Some((head, tail)) if Date::parse(tail).is_ok() => head.to_string(),
        _ => id.to_string(),
    }
}

type Row = (String, String, u32, Vec<(String, (f64, f64, String), u32, u32)>);

fn rows(net: &Network, d: Date) -> Vec<Row> {
    let mut out: Vec<Row> = materialize(net, d)
        .unwrap()
        .into_iter()
        .map(|t| {
            let visits = t
                .stop_times
                .iter()
                .map(|st| {
                    let s = &net.stops[&st.stop];
                    (base(&st.stop), (s.lat, s.lon, s.name.clone()), st.arrival_secs, st.departure_secs)
                })
                .collect();
            (base(&t.trip), base(&t.pattern), t.start_secs, visits)
        })
        .collect();
    out.sort_by(|a, b| (&a.0, a.2, &a.1).cmp(&(&b.0, b.2, &b.1)));
    out
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// On every date the export runs the trips the pick runs, each visit at
    /// its stop's record of that day.
    #[test]
    fn every_date_of_the_export_is_the_pick_on_that_date(stops in arb_stops(&fixture()), from in 0u32..40, len in proptest::option::of(5u32..60), d in 0u32..110) {
        let net = fixture();
        let until = len.map(|l| day(from + l));
        let exported = export_versions(&net, &stops, day(from), until);
        let on = day(d).max(day(from));
        let on = until.map_or(on, |u| on.min(u.pred_opt().unwrap()));
        prop_assert_eq!(rows(&exported, day(d)), rows(&read_records(&net, &stops, on), day(d)), "on {}", date(d));
    }

    /// Each stop is one row per version, all with the stop's code. A stop
    /// whose record does not change in the window is one row.
    #[test]
    fn a_stop_is_one_row_per_version_sharing_its_code(stops in arb_stops(&fixture()), from in 0u32..40, len in proptest::option::of(5u32..60)) {
        let net = fixture();
        let until = len.map(|l| day(from + l));
        let exported = export_versions(&net, &stops, day(from), until);
        for (id, s) in &exported.stops {
            let own = base(id);
            prop_assert!(net.stops.contains_key(&own), "{} is a version of a stop the pick has", id);
            if id != &own {
                prop_assert!(exported.stops.contains_key(&own), "the last version keeps the id");
                prop_assert_eq!(s.info.code.clone(), exported.stops[&own].info.code.clone(), "the versions share a code");
            }
        }
        let versioned = exported.stops.keys().any(|id| *id != base(id));
        if !versioned {
            prop_assert_eq!(exported, read_records(&net, &stops, day(from)));
        }
    }

    /// A re-import of the export adds no event. It names as unapplied
    /// exactly the exported stops that are retired on the first date.
    #[test]
    fn a_reimport_of_the_export_adds_no_event(stops in arb_stops(&fixture()), from in 0u32..40, len in proptest::option::of(5u32..60)) {
        let net = fixture();
        let until = len.map(|l| day(from + l));
        let exported = export_versions(&net, &stops, day(from), until);
        let imported = import_events(&stops, &exported, day(from));
        prop_assert!(imported.added.is_empty(), "{:?}", imported.added);
        let retired: Vec<String> = exported
            .stops
            .keys()
            .filter(|id| !id.contains('@'))
            .filter(|id| stops.stop_on(id, day(from)).is_some_and(|s| s.status == transit_stops::Status::Retired))
            .cloned()
            .collect();
        prop_assert_eq!(&imported.unapplied, &retired);
        prop_assert_eq!(&imported.stops, &stops);
    }

    /// A feed's difference is an event dated its first date. Every event
    /// already recorded stays, and the document still passes `checked`.
    #[test]
    fn a_feed_difference_is_an_event_on_its_first_date(stops in arb_stops(&fixture()), on in 0u32..80, moved in 1i32..300, renamed in any::<bool>()) {
        let mut feed = read_records(&fixture(), &stops, day(on));
        let first = feed.stops.keys().next().unwrap().clone();
        feed.stops.get_mut(&first).unwrap().lat += moved as f64 * 1e-5;
        if renamed {
            feed.stops.get_mut(&first).unwrap().name = "From the feed".into();
        }
        let imported = import_events(&stops, &feed, day(on));
        prop_assert!(imported.stops.clone().checked().is_ok());
        for (id, events) in &stops.stops {
            let now = &imported.stops.stops[id];
            prop_assert!(events.iter().all(|e| now.contains(e)), "stop {} keeps its events", id);
        }
        for a in &imported.added {
            prop_assert_eq!(a.event.on.clone(), Some(date(on)));
        }
        if !imported.unapplied.contains(&first) {
            let s = imported.stops.stop_on(&first, day(on)).unwrap();
            prop_assert_eq!(s.lat, feed.stops[&first].lat);
            if renamed {
                prop_assert_eq!(s.details.name.as_deref(), Some("From the feed"));
            }
        }
    }
}

#[test]
fn a_move_inside_the_window_is_two_rows_and_a_calendar_split() {
    let net = fixture();
    let mut stops = seed(&net);
    let b = stops.stops["B"][0].clone();
    stops = stops.add_event("B", Event { lat: Some(b.lat.unwrap() + 0.001), lon: b.lon, ..ev(EventKind::Moved, 30) });
    let stops = stops.checked().unwrap();
    let exported = export_versions(&net, &stops, day(0), Some(day(91)));
    assert_eq!(exported.stops["B@2026-09-01"].lat, b.lat.unwrap(), "the earlier version under a made id");
    assert_eq!(exported.stops["B"].lat, b.lat.unwrap() + 0.001, "the version in force on the last date keeps the id");
    let visits = |d: u32| -> Vec<String> {
        materialize(&exported, day(d)).unwrap().iter().flat_map(|t| t.stop_times.iter().map(|s| s.stop.clone())).filter(|s| base(s) == "B").collect()
    };
    assert!(!visits(29).is_empty() && visits(29).iter().all(|s| s == "B@2026-09-01"));
    assert!(!visits(30).is_empty() && visits(30).iter().all(|s| s == "B"));
    let imported = import_events(&stops, &exported, day(0));
    assert!(imported.added.is_empty() && imported.unlisted.is_empty());
}

#[test]
fn a_suspension_splits_nothing() {
    let net = fixture();
    let stops = seed(&net).add_event("B", Event { until: Some(date(40)), ..ev(EventKind::Suspended, 30) }).checked().unwrap();
    assert_eq!(export_versions(&net, &stops, day(0), Some(day(91))), read_records(&net, &stops, day(0)));
}

#[test]
fn an_import_records_a_new_stop_and_names_the_ones_it_does_not_list() {
    let net = fixture();
    let mut stops = seed(&net);
    stops.stops.remove("C");
    stops.stops.insert("Z".into(), vec![Event { lat: Some(45.0), lon: Some(-122.0), ..Event::default() }]);
    let stops = stops.checked().unwrap();
    let imported = import_events(&stops, &net, day(5));
    assert_eq!(imported.unlisted, vec!["Z".to_string()], "Z keeps its events; the feed does not list it");
    let c = imported.added.iter().find(|a| a.stop == "C").expect("C is added");
    assert_eq!((c.event.event.clone(), c.event.on.clone()), (EventKind::InService, Some(date(5))));
    assert_eq!(imported.stops.stops["Z"], stops.stops["Z"]);
}

/// Only a stop in service or suspended on the feed's first date is named.
#[test]
fn unlisted_names_only_the_stops_a_feed_should_list() {
    let net = fixture();
    let mut stops = seed(&net);
    let at = |kind: EventKind, on: u32| Event { lat: Some(45.0), lon: Some(-122.0), ..ev(kind, on) };
    stops.stops.insert("P".into(), vec![at(EventKind::Planned, 1)]);
    stops.stops.insert("R".into(), vec![at(EventKind::InService, 1), ev(EventKind::Retired, 3)]);
    stops.stops.insert("S".into(), vec![at(EventKind::InService, 1), ev(EventKind::Suspended, 3)]);
    stops.stops.insert("L".into(), vec![at(EventKind::Planned, 20)]);
    stops.stops.insert("Z".into(), vec![Event { lat: Some(45.0), lon: Some(-122.0), ..Event::default() }]);
    let stops = stops.checked().unwrap();
    let imported = import_events(&stops, &net, day(5));
    assert_eq!(imported.unlisted, vec!["S".to_string(), "Z".to_string()]);
}

/// `import_feed` equals `import_events` apart from `from_feed`.
#[test]
fn an_import_names_its_feed_on_every_event_it_records() {
    let net = fixture();
    let mut stops = seed(&net);
    stops.stops.remove("C");
    let a = stops.stops.get_mut("A").unwrap();
    a[0].details.name = Some("Old A".into());
    let stops = stops.checked().unwrap();
    let plain = import_events(&stops, &net, day(5));
    let named = import_feed(&stops, &net, day(5), "sha256:feed");
    assert!(!named.added.is_empty());
    assert!(named.added.iter().all(|a| a.event.from_feed.as_deref() == Some("sha256:feed")));
    let strip = |events: &[AddedEvent]| events.iter().map(|a| AddedEvent { event: Event { from_feed: None, ..a.event.clone() }, ..a.clone() }).collect::<Vec<_>>();
    assert_eq!(strip(&named.added), plain.added);
    assert_eq!((named.unlisted.clone(), named.unapplied.clone()), (plain.unlisted, plain.unapplied));
    assert_eq!(feed_events(&named.stops, "sha256:feed"), named.added);
    assert!(feed_events(&named.stops, "sha256:other").is_empty());
    assert!(feed_events(&stops, "sha256:feed").is_empty());
}

/// A feed that lists a retired stop's id is named in `unapplied`, whether or
/// not its point matches, and nothing is added.
#[test]
fn a_feed_that_lists_a_retired_stop_names_it_unapplied() {
    let net = fixture();
    let stops = seed(&net).add_event("B", ev(EventKind::Retired, 10)).checked().unwrap();
    let mut feed = read_records(&net, &stops, day(0));
    let same = import_events(&stops, &feed, day(20));
    assert_eq!((same.added.clone(), same.unapplied.clone()), (vec![], vec!["B".to_string()]), "the same point and details");
    feed.stops.get_mut("B").unwrap().lat += 0.001;
    let moved = import_events(&stops, &feed, day(20));
    assert_eq!((moved.added.iter().filter(|a| a.stop == "B").count(), moved.unapplied.clone()), (0, vec!["B".to_string()]), "another point");
    let before = import_events(&stops, &read_records(&net, &stops, day(0)), day(5));
    assert!(before.unapplied.is_empty(), "before its retirement the stop is in service: {:?}", before.unapplied);
}
