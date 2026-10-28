//! Where a bus is going, as its sign says it: the visit's own
//! headsign (GTFS stop_headsign overrides the trip's at that visit
//! only), else the trip's headsign, else the final stop — named as
//! the PLACE (its parent station), never the bay it pulls into.

use transit_network::{materialize, visit_destination, Network, Stop};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap()
}

fn date() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()
}

fn destinations(net: &Network) -> Vec<String> {
    let day = materialize(net, date()).unwrap();
    let trip = day.iter().find(|t| t.trip == "10-weekday@28800").unwrap();
    (0..trip.stop_times.len()).map(|i| visit_destination(net, trip, i)).collect()
}

#[test]
fn an_unsigned_trip_is_going_to_its_final_stop() {
    assert_eq!(destinations(&network()), ["Main & 3rd", "Main & 3rd", "Main & 3rd"]);
}

#[test]
fn a_final_bay_is_named_by_its_station() {
    let mut net = network();
    net.stops.insert("STN".into(), Stop { name: "Main Transit Center".into(), ..Default::default() });
    net.stops.get_mut("C").unwrap().parent = Some("STN".into());
    net.stops.get_mut("C").unwrap().name = "Main Transit Center: Bay D".into();
    assert_eq!(destinations(&net)[0], "Main Transit Center");
}

#[test]
fn the_trip_headsign_beats_the_final_stop() {
    let mut net = network();
    net.pattern_trips.get_mut("10-weekday").unwrap().trips[0].headsign = Some("Downtown".into());
    assert_eq!(destinations(&net), ["Downtown", "Downtown", "Downtown"]);
}

#[test]
fn a_visit_headsign_overrides_the_trip_at_that_visit_only() {
    let mut net = network();
    net.pattern_trips.get_mut("10-weekday").unwrap().trips[0].headsign = Some("Downtown".into());
    net.patterns.get_mut("10-inbound").unwrap().stops[0].headsign = Some("Downtown via Main".into());
    assert_eq!(destinations(&net), ["Downtown via Main", "Downtown", "Downtown"]);
}

#[test]
fn a_loop_reads_the_headsign_of_the_visit_it_is_on() {
    // A → B → A: the first A says where the loop goes, the second
    // (the terminal) says nothing of its own.
    let mut net = network();
    let pattern = net.patterns.get_mut("10-inbound").unwrap();
    pattern.stops[2].stop = "A".into();
    pattern.stops[0].headsign = Some("Loop via B".into());
    let day = materialize(&net, date()).unwrap();
    let trip = day.iter().find(|t| t.trip == "10-weekday@28800").unwrap();
    assert_eq!(visit_destination(&net, trip, 0), "Loop via B");
    assert_eq!(visit_destination(&net, trip, 2), "Main & 1st");
}
