//! A network has one timezone or none. GTFS requires every agency in a
//! dataset to share one `agency_timezone`. Operators that disagree make an
//! invalid dataset, so the network names no zone.

use transit_network::{operators::Operator, Network};

fn operator(timezone: &str) -> Operator {
    Operator {
        name: "Op".into(),
        url: "https://example.com".into(),
        timezone: timezone.into(),
        ..Operator::default()
    }
}

fn network(zones: &[&str]) -> Network {
    let mut network = Network::default();
    for (i, zone) in zones.iter().enumerate() {
        network.operators.insert(format!("op{i}"), operator(zone));
    }
    network
}

#[test]
fn one_operator_names_the_networks_zone() {
    assert_eq!(
        network(&["America/Los_Angeles"]).timezone(),
        Some("America/Los_Angeles")
    );
}

#[test]
fn operators_that_agree_name_one_zone() {
    assert_eq!(
        network(&["America/Los_Angeles", "America/Los_Angeles"]).timezone(),
        Some("America/Los_Angeles")
    );
}

#[test]
fn no_operator_or_an_empty_zone_names_none() {
    assert_eq!(network(&[]).timezone(), None);
    assert_eq!(network(&[""]).timezone(), None);
}

#[test]
fn operators_that_disagree_name_none() {
    assert_eq!(
        network(&["America/Los_Angeles", "America/New_York"]).timezone(),
        None
    );
    assert_eq!(network(&["America/Los_Angeles", ""]).timezone(), None);
}
