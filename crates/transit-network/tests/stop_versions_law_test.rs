//! GTFS out and in with the stops' history (D-150, D-151, D-157) — the
//! laws, from the user's intent, before the implementation:
//!
//! * EXPORT: a stop whose record (point, name, code, details) changes
//!   inside the export's dates is one stop per version — the version in
//!   force on the last date under the stop's id, each earlier one under
//!   `<id>@<its first date>`, every one with the stop's code — and each
//!   pattern visiting it splits at those dates, its trips under calendars
//!   cut to each version's dates; on every date the export runs exactly
//!   the trips the pick runs that day, at the stops' records of that day;
//!   a suspension or retirement splits nothing; with no record changing,
//!   the export is the pick read on its first date;
//! * IMPORT: the feed's stops compared with the stops document on the
//!   feed's first date, each difference an event dated that day
//!   (`in_service` for a stop the document lacks or has not yet in
//!   service, `moved`, `changed`); a stop the feed does not list keeps
//!   its events, later events stand, a made id reads back as a version of
//!   its stop — so a re-import of this repo's own export adds no event.

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_network::stop_record::{export_versions, import_events, read_records, seed};
use transit_network::{materialize, Network};
use transit_stops::{Details, Event, EventKind, Stops};

fn fixture() -> Network {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    let load: serde_json::Value = serde_json::from_str(&text).unwrap();
    serde_json::from_value(load["network"].clone()).unwrap()
}

/// The fixture runs its weekday and Sunday service through 2026.
fn day(n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 1).unwrap() + chrono::Days::new(n as u64)
}

fn date(n: u32) -> String {
    day(n).format("%Y-%m-%d").to_string()
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

/// The fixture's stops seeded, some given up to two changes.
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

/// An exported id back to its stop's, pattern's, or trip's own: the
/// `@<date>` a version or a split adds, taken off.
fn base(id: &str) -> String {
    match id.rsplit_once('@') {
        Some((head, tail)) if NaiveDate::parse_from_str(tail, "%Y-%m-%d").is_ok() => head.to_string(),
        _ => id.to_string(),
    }
}

/// A day's trips as comparable rows: trip, start, and each visit's stop
/// (its own id, and the record it is exported under) and times.
type Row = (String, String, u32, Vec<(String, (f64, f64, String), u32, u32)>);

fn rows(net: &Network, d: NaiveDate) -> Vec<Row> {
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

    /// On every date the export runs the trips the pick runs, each visit
    /// at its stop's record of that day (of the first date, before it).
    #[test]
    fn every_date_of_the_export_is_the_pick_on_that_date(stops in arb_stops(&fixture()), from in 0u32..40, len in proptest::option::of(5u32..60), d in 0u32..110) {
        let net = fixture();
        let until = len.map(|l| day(from + l));
        let exported = export_versions(&net, &stops, day(from), until);
        let on = day(d).max(day(from));
        let on = until.map_or(on, |u| on.min(u.pred_opt().unwrap()));
        prop_assert_eq!(rows(&exported, day(d)), rows(&read_records(&net, &stops, on), day(d)), "on {}", date(d));
    }

    /// Each stop is one stop per version: its own id last, the others
    /// made from it and their first date, all with the stop's code; a
    /// stop whose record does not change in the window is one stop.
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

    /// A re-import of this repo's own export adds no event, and names as
    /// unlisted only the stops the export does not carry.
    #[test]
    fn a_reimport_of_the_export_adds_no_event(stops in arb_stops(&fixture()), from in 0u32..40, len in proptest::option::of(5u32..60)) {
        let net = fixture();
        let until = len.map(|l| day(from + l));
        let exported = export_versions(&net, &stops, day(from), until);
        let imported = import_events(&stops, &exported, day(from));
        prop_assert!(imported.added.is_empty(), "{:?}", imported.added);
        prop_assert!(imported.unapplied.is_empty(), "{:?}", imported.unapplied);
        prop_assert_eq!(&imported.stops, &stops);
    }

    /// A feed's difference is an event dated its first date: the stop then
    /// reads as the feed has it; every event already recorded stays, and the
    /// document still passes what a write must pass.
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
    // B moves on 2026-10-01, inside an export from 2026-09-01 to 2026-12-01
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
    // the made id reads back as a version of B
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
