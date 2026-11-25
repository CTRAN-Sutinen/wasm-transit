//! The seed of a stops document from a network.

use transit_stops::stop_record::seed;
use transit_network::Network;
use transit_stops::Status;

fn fixture() -> Network {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    let load: serde_json::Value = serde_json::from_str(&text).unwrap();
    serde_json::from_value(load["network"].clone()).unwrap()
}

#[test]
fn the_seed_records_every_stop_in_service_as_the_network_has_it() {
    let net = fixture();
    let stops = seed(&net).checked().expect("a seed passes what a write must pass");
    assert_eq!(stops.stops.keys().collect::<Vec<_>>(), net.stops.keys().collect::<Vec<_>>());
    let any_date = chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap();
    for (id, stop) in &net.stops {
        let on = stops.stop_on(id, any_date).expect("there from before any record");
        assert_eq!(on.status, Status::InService);
        assert_eq!((on.lat, on.lon), (stop.lat, stop.lon));
        assert_eq!(on.details.name.as_deref().unwrap_or(""), stop.name);
        assert_eq!(on.details.code, stop.info.code);
        assert_eq!(on.details.parent, stop.parent);
    }
}
