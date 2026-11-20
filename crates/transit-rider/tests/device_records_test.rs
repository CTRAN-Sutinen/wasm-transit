//! A rider's device records (docs/FORMATS.md, "Versions"): the recent
//! places and the rider's own live-feed URLs, each a tagged document in
//! the browser's document store. Each writes its kind and version, reads
//! a copy with neither as v1, refuses another kind, and keeps a field
//! this build does not know (D-167).

use serde_json::json;
use transit_rider::device::{LiveFeeds, RecentPlaces};
use transit_version::Versioned;

#[test]
fn recent_places_write_their_tags_and_keep_what_they_do_not_know() {
    let read: RecentPlaces = serde_json::from_value(json!({
        "places": [{"id": "A", "name": "Main St", "lat": 45.5, "lon": -122.6, "town": "Vancouver"}],
        "pinned": true
    }))
    .unwrap();
    assert_eq!(read.schema_version(), 1);
    assert_eq!(
        serde_json::to_value(&read).unwrap(),
        json!({
            "kind": "recent_places", "schema_version": 1,
            "places": [{"id": "A", "name": "Main St", "lat": 45.5, "lon": -122.6, "town": "Vancouver"}],
            "pinned": true
        })
    );
    // A place without a point (a stop the rider found by name) has none.
    let bare: RecentPlaces = serde_json::from_value(json!({"places": [{"id": "B", "name": "Elm"}]})).unwrap();
    assert_eq!(serde_json::to_value(&bare).unwrap()["places"], json!([{"id": "B", "name": "Elm"}]));
    assert!(serde_json::from_value::<RecentPlaces>(json!({"kind": "live_feeds", "places": []})).is_err());
}

#[test]
fn live_feeds_write_their_tags_and_read_an_absent_feed_as_none() {
    let read: LiveFeeds = serde_json::from_value(json!({"trips": "https://agency.example/tu.pb"})).unwrap();
    assert_eq!(read.schema_version(), 1);
    assert_eq!(
        serde_json::to_value(&read).unwrap(),
        json!({"kind": "live_feeds", "schema_version": 1, "trips": "https://agency.example/tu.pb", "vehicles": "", "alerts": ""})
    );
    assert!(serde_json::from_value::<LiveFeeds>(json!({"kind": "recent_places"})).is_err());
}
