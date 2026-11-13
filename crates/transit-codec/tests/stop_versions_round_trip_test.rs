//! GTFS out and back in with the stops' history (D-150, D-151) — the law
//! at the codec's boundary: a pick exported with a stop moved inside its
//! dates is a GTFS feed with one stops.txt row per version sharing the
//! stop's code, and that feed, imported again, adds no event to the stops
//! document it was made from (a made id reads back as a version).

use chrono::NaiveDate;
use transit_network::stop_record::{export_versions, import_events, seed};
use transit_network::Network;
use transit_stops::{Event, EventKind};

fn fixture() -> Network {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    let load: serde_json::Value = serde_json::from_str(&text).unwrap();
    serde_json::from_value(load["network"].clone()).unwrap()
}

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

#[test]
fn a_feed_exported_with_the_stops_history_reimports_with_no_new_event() {
    let net = fixture();
    let b = &net.stops["B"];
    let stops = seed(&net)
        .add_event("B", Event { on: Some("2026-10-15".into()), event: EventKind::Moved, lat: Some(b.lat + 0.001), lon: Some(b.lon), ..Event::default() })
        .checked()
        .unwrap();
    let exported = export_versions(&net, &stops, d("2026-10-01"), Some(d("2026-12-01")));
    let feed = transit_codec::export(&exported, &Default::default()).unwrap();
    let rows: Vec<_> = feed.stops.iter().filter(|s| matches!(s.stop_id.as_deref(), Some("B" | "B@2026-10-01"))).collect();
    assert_eq!(rows.len(), 2, "one stops.txt row per version");
    assert_eq!(rows[0].stop_code, rows[1].stop_code, "sharing the stop's code");

    let back = transit_codec::import(&transit_codec::Feed::from_zip_bytes(&feed.to_zip_bytes()).unwrap()).network;
    let imported = import_events(&stops, &back, d("2026-10-01"));
    assert!(imported.added.is_empty(), "{:?}", imported.added);
    assert!(imported.unapplied.is_empty() && imported.unlisted.is_empty());
}
