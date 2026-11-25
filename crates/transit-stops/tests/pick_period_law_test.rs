//! A pick's period, and what a pick reads of the stops over it (D-157,
//! D-228). The ops are in cap-stops' OPS.md (`read_pick`, `export_pick`).

use chrono::NaiveDate;
use transit_network::Network;
use transit_plans::PublishedPlans;
use transit_stops::stop_record::{export_pick, export_versions, pick_period, read_pick, read_records, seed, visit_warnings};
use transit_stops::{Event, EventKind};

fn fixture() -> Network {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    let load: serde_json::Value = serde_json::from_str(&text).unwrap();
    serde_json::from_value(load["network"].clone()).unwrap()
}

fn pick(id: &str, from: Option<&str>) -> Network {
    let mut net = fixture();
    net.plan.id = id.into();
    net.plan.name = id.into();
    net.plan.effective_from = from.map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap());
    net
}

fn published(entries: &[(&str, &str, &str)]) -> PublishedPlans {
    let plans: Vec<_> = entries
        .iter()
        .map(|(id, from, status)| serde_json::json!({"id": id, "name": id, "effective_from": from, "network_hash": "sha256:x", "status": status}))
        .collect();
    serde_json::from_value(serde_json::json!({"kind": "plans", "schema_version": 1, "plans": plans})).unwrap()
}

#[test]
fn a_pick_governs_until_the_next_pick_that_is_not_retired() {
    let set = published(&[("a", "2027-01-10", "published"), ("b", "2027-03-01", "retired"), ("c", "2027-06-01", "published")]);
    let period = pick_period(&pick("a", Some("2027-01-10")), Some(&set)).unwrap();
    assert_eq!((period.from.as_str(), period.until.as_deref()), ("2027-01-10", Some("2027-06-01")));
}

#[test]
fn the_last_pick_and_a_first_publish_are_open_ended() {
    let set = published(&[("a", "2027-01-10", "published")]);
    assert_eq!(pick_period(&pick("a", Some("2027-01-10")), Some(&set)).unwrap().until, None);
    assert_eq!(pick_period(&pick("a", Some("2027-01-10")), None).unwrap().until, None);
}

#[test]
fn the_pick_is_placed_by_its_id_and_its_own_date_wins() {
    let set = published(&[("a", "2027-01-10", "published"), ("b", "2027-02-01", "published"), ("c", "2027-06-01", "published")]);
    let period = pick_period(&pick("b", Some("2027-09-01")), Some(&set)).unwrap();
    assert_eq!((period.from.as_str(), period.until.as_deref()), ("2027-09-01", None));
    let set = published(&[("a", "2027-01-10", "published"), ("b", "2027-02-01", "retired")]);
    assert_eq!(pick_period(&pick("b", Some("2027-01-20")), Some(&set)).unwrap().until, None);
}

#[test]
fn a_pick_with_no_id_or_no_date_has_no_period_and_reads_nothing() {
    let stops = seed(&fixture());
    for net in [pick("", Some("2027-01-10")), pick("a", None)] {
        assert_eq!(pick_period(&net, None), None);
        assert!(read_pick(&net, &stops, None).is_none());
        assert!(export_pick(&net, &stops, None).is_none());
    }
}

#[test]
fn a_pick_reads_the_stops_over_its_period() {
    let net = pick("a", Some("2027-01-10"));
    let mut stops = seed(&net);
    let id = net.stops.keys().next().unwrap().clone();
    stops = stops.add_event(&id, Event { on: Some("2027-02-01".into()), event: EventKind::Retired, ..Event::default() });
    let stops = stops.checked().unwrap();
    let set = published(&[("a", "2027-01-10", "published"), ("c", "2027-06-01", "published")]);
    let from = NaiveDate::from_ymd_opt(2027, 1, 10).unwrap();
    let until = NaiveDate::from_ymd_opt(2027, 6, 1).unwrap();

    let read = read_pick(&net, &stops, Some(&set)).unwrap();
    assert_eq!(read.network, read_records(&net, &stops, from));
    assert_eq!(read.warnings, visit_warnings(&net, &stops, from, Some(until)));
    assert!(!read.warnings.is_empty(), "the retirement is inside the period");
    assert_eq!((read.period.from.as_str(), read.period.until.as_deref()), ("2027-01-10", Some("2027-06-01")));

    assert_eq!(export_pick(&net, &stops, Some(&set)).unwrap(), export_versions(&net, &stops, from, Some(until)));
}
