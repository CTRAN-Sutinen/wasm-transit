//! `stops_near` and `stops_near_walked` (D-34; cap-directory/OPS.md,
//! `stops_near`). The expected answers come from the intent, not the
//! implementation.
//!
//! 1. Rows are ordered by great-circle distance in meters, ties by stop id.
//! 2. With a radius, every row lies within it, and every place within it
//!    answers, up to the limit.
//! 3. The nearest place is first, at the distance of its nearest point.
//! 4. There are never more rows than the limit.
//! 5. A station's bays answer as one row, the station, at the distance of the
//!    nearest member. A stop with no station is its own place.
//!
//! With the point's walks to stops (cap-streets `access`):
//!
//! 6. The places a walk reaches come first, by walk, then distance, then id.
//!    The rest follow by distance.
//! 7. A place's walk is the shortest walk to any member. A walk to a stop that
//!    the network lacks reaches nothing.
//! 8. The walks reorder the rows and change nothing else.

use std::collections::BTreeSet;

use proptest::prelude::*;
use transit_network::listing::{stops_near, stops_near_walked, NearbyStop};
use transit_network::{Network, Stop};

/// The spec's formula with the mean earth radius, independent of the library.
fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6_371_000.0 * a.sqrt().asin()
}

fn stop(name: &str, lat: f64, lon: f64, parent: Option<&str>) -> Stop {
    Stop { name: name.into(), lat, lon, parent: parent.map(Into::into), ..Default::default() }
}

fn place_of<'a>(net: &'a Network, id: &'a str) -> &'a str {
    match net.stops[id].parent.as_deref() {
        Some(p) if net.stops.contains_key(p) => p,
        _ => id,
    }
}

fn place_distances(net: &Network, lat: f64, lon: f64) -> std::collections::BTreeMap<String, f64> {
    let mut out: std::collections::BTreeMap<String, f64> = Default::default();
    for (id, s) in &net.stops {
        let d = haversine_m(lat, lon, s.lat, s.lon);
        let e = out.entry(place_of(net, id).to_string()).or_insert(f64::INFINITY);
        *e = e.min(d);
    }
    out
}

fn arb_network() -> impl Strategy<Value = Network> {
    prop::collection::vec(
        (0.0f64..0.02, 0.0f64..0.02, 0u8..4, any::<prop::sample::Index>()),
        1..30,
    )
    .prop_map(|raw| {
        let mut net = Network::default();
        let stations: Vec<String> =
            raw.iter().enumerate().filter(|(_, r)| r.2 == 0).map(|(i, _)| format!("S{i:02}")).collect();
        for (i, (dlat, dlon, kind, pick)) in raw.iter().enumerate() {
            let (lat, lon) = (45.6 + dlat, -122.6 + dlon);
            let (id, s) = match kind {
                0 => (format!("S{i:02}"), stop("Station", lat, lon, None)),
                1 if !stations.is_empty() => {
                    let parent = &stations[pick.index(stations.len())];
                    (format!("B{i:02}"), stop("Bay", lat, lon, Some(parent)))
                }
                2 => (format!("O{i:02}"), stop("Orphan bay", lat, lon, Some("GONE"))),
                _ => (format!("P{i:02}"), stop("Plain", lat, lon, None)),
            };
            net.stops.insert(id, s);
        }
        net
    })
}

fn arb_point() -> impl Strategy<Value = (f64, f64)> {
    (45.59f64..45.63, -122.61f64..-122.57)
}

proptest! {
    #[test]
    fn rows_are_ordered_by_distance_ties_by_id(
        net in arb_network(), (lat, lon) in arb_point(), limit in 0usize..40,
        radius in prop::option::of(0.0f64..3000.0),
    ) {
        let rows = stops_near(&net, lat, lon, limit, radius);
        for w in rows.windows(2) {
            prop_assert!(
                w[0].distance_m < w[1].distance_m
                    || (w[0].distance_m == w[1].distance_m && w[0].stop_id < w[1].stop_id),
                "{:?} then {:?}", w[0], w[1]
            );
        }
    }

    #[test]
    fn every_row_is_within_the_radius_and_every_place_within_it_answers(
        net in arb_network(), (lat, lon) in arb_point(), radius in 0.0f64..3000.0,
    ) {
        let rows = stops_near(&net, lat, lon, usize::MAX, Some(radius));
        prop_assert!(rows.iter().all(|r| r.distance_m <= radius));
        let within: BTreeSet<String> = place_distances(&net, lat, lon)
            .into_iter()
            .filter(|(_, d)| *d <= radius)
            .map(|(id, _)| id)
            .collect();
        let answered: BTreeSet<String> = rows.iter().map(|r| r.stop_id.clone()).collect();
        prop_assert_eq!(answered, within);
    }

    #[test]
    fn the_nearest_place_is_first_at_its_nearest_members_distance(
        net in arb_network(), (lat, lon) in arb_point(), limit in 1usize..10,
    ) {
        let rows = stops_near(&net, lat, lon, limit, None);
        let places = place_distances(&net, lat, lon);
        let nearest = places.values().cloned().fold(f64::INFINITY, f64::min);
        prop_assert!((rows[0].distance_m - nearest).abs() < 1e-6, "{} vs {}", rows[0].distance_m, nearest);
        for r in &rows {
            prop_assert!((r.distance_m - places[&r.stop_id]).abs() < 1e-6, "{:?}", r);
        }
    }

    #[test]
    fn never_more_rows_than_the_limit_nor_than_places(
        net in arb_network(), (lat, lon) in arb_point(), limit in 0usize..40,
    ) {
        let rows = stops_near(&net, lat, lon, limit, None);
        let places = place_distances(&net, lat, lon).len();
        prop_assert_eq!(rows.len(), limit.min(places));
    }

    #[test]
    fn one_row_per_place_a_bay_answers_as_its_station(
        net in arb_network(), (lat, lon) in arb_point(),
    ) {
        let rows = stops_near(&net, lat, lon, usize::MAX, None);
        let ids: BTreeSet<&str> = rows.iter().map(|r| r.stop_id.as_str()).collect();
        prop_assert_eq!(ids.len(), rows.len(), "no place twice");
        for r in &rows {
            prop_assert_eq!(place_of(&net, &r.stop_id), r.stop_id.as_str(), "a row is a place, never a bay of one");
            let s = &net.stops[&r.stop_id];
            prop_assert_eq!(&r.name, &s.name);
            prop_assert_eq!((r.lat, r.lon), (s.lat, s.lon));
            prop_assert_eq!(&r.parent, &s.parent);
            prop_assert_eq!(r.transfer, s.transfer);
        }
    }
}

fn arb_walks(net: &Network) -> impl Strategy<Value = Vec<(String, u32)>> {
    let mut ids: Vec<String> = net.stops.keys().cloned().collect();
    ids.push("NOWHERE".into());
    prop::collection::vec((prop::sample::select(ids), 0u32..600), 0..12)
}

fn case() -> impl Strategy<Value = (Network, Vec<(String, u32)>)> {
    arb_network().prop_flat_map(|net| {
        let walks = arb_walks(&net);
        (Just(net), walks)
    })
}

fn place_walks(net: &Network, walks: &[(String, u32)]) -> std::collections::BTreeMap<String, u32> {
    let mut out: std::collections::BTreeMap<String, u32> = Default::default();
    for (id, secs) in walks {
        if !net.stops.contains_key(id) {
            continue;
        }
        let e = out.entry(place_of(net, id).to_string()).or_insert(u32::MAX);
        *e = (*e).min(*secs);
    }
    out
}

proptest! {
    #[test]
    fn walked_places_come_first_by_walk_then_the_rest_by_distance(
        (net, walks) in case(), (lat, lon) in arb_point(), limit in 0usize..40,
        radius in prop::option::of(0.0f64..3000.0),
    ) {
        let rows = stops_near_walked(&net, lat, lon, limit, radius, &walks);
        let key = |r: &NearbyStop| (r.walk_secs.is_none(), r.walk_secs.unwrap_or(0));
        for w in rows.windows(2) {
            let (a, b) = (key(&w[0]), key(&w[1]));
            prop_assert!(
                a < b || (a == b && (w[0].distance_m < w[1].distance_m
                    || (w[0].distance_m == w[1].distance_m && w[0].stop_id < w[1].stop_id))),
                "{:?} then {:?}", w[0], w[1]
            );
        }
    }

    #[test]
    fn a_place_walks_as_far_as_its_nearest_member(
        (net, walks) in case(), (lat, lon) in arb_point(),
    ) {
        let want = place_walks(&net, &walks);
        for r in stops_near_walked(&net, lat, lon, usize::MAX, None, &walks) {
            prop_assert_eq!(r.walk_secs, want.get(&r.stop_id).copied(), "{:?}", r);
        }
    }

    #[test]
    fn the_walks_reorder_the_rows_and_nothing_else(
        (net, walks) in case(), (lat, lon) in arb_point(), radius in prop::option::of(0.0f64..3000.0),
    ) {
        let walked = stops_near_walked(&net, lat, lon, usize::MAX, radius, &walks);
        let plain = stops_near(&net, lat, lon, usize::MAX, radius);
        prop_assert!(plain.iter().all(|r| r.walk_secs.is_none()), "no walks, no walk times");
        let set = |rows: &[NearbyStop]| rows.iter().map(|r| (r.stop_id.clone(), r.distance_m.to_bits())).collect::<BTreeSet<_>>();
        prop_assert_eq!(set(&walked), set(&plain));
    }
}

/// A station whose bays sit nearer than its pin answers once, as the station,
/// at its nearest bay's distance.
#[test]
fn a_station_answers_once_at_its_nearest_bay() {
    let mut net = Network::default();
    net.stops.insert("STN".into(), stop("Fisher's Landing TC", 45.6100, -122.5000, None));
    net.stops.insert("STN-A".into(), stop("Fisher's Landing TC Bay A", 45.6010, -122.5000, Some("STN")));
    net.stops.insert("STN-B".into(), stop("Fisher's Landing TC Bay B", 45.6020, -122.5000, Some("STN")));
    net.stops.insert("P".into(), Stop { transfer: true, ..stop("Mill Plain & 164th", 45.6050, -122.5000, None) });

    let rows: Vec<NearbyStop> = stops_near(&net, 45.6000, -122.5000, 10, None);
    let ids: Vec<&str> = rows.iter().map(|r| r.stop_id.as_str()).collect();
    assert_eq!(ids, ["STN", "P"]);
    assert_eq!(rows[0].name, "Fisher's Landing TC");
    assert!((rows[0].distance_m - haversine_m(45.6, -122.5, 45.601, -122.5)).abs() < 1e-6);
    assert!(rows[1].transfer, "the row carries the stop's transfer flag");

    assert!(stops_near(&net, 45.6000, -122.5000, 10, Some(50.0)).is_empty());
    assert!(stops_near(&Network::default(), 45.6, -122.5, 10, None).is_empty());
}
