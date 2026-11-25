//! The final stop is named as its place (its parent station), not as the
//! bay that the trip pulls into.

use crate::materialize::MaterializedTrip;
use crate::model::Network;

/// The headsign of the visit overrides the headsign of the trip at that
/// visit only. A patched trip that skips stops still reads the headsigns of
/// its pattern, because a visit matches its pattern stop by stop and
/// occurrence (a loop visits a stop twice).
pub fn visit_destination(network: &Network, trip: &MaterializedTrip, visit: usize) -> String {
    visit_headsign(network, trip, visit)
        .or_else(|| {
            network
                .pattern_trips
                .get(&trip.pattern_trips)
                .and_then(|pt| pt.trips.iter().find(|t| t.id == trip.trip))
                .and_then(|t| t.headsign.clone())
        })
        .unwrap_or_else(|| final_place(network, trip))
}

fn visit_headsign(network: &Network, trip: &MaterializedTrip, visit: usize) -> Option<String> {
    let stop = &trip.stop_times.get(visit)?.stop;
    let occurrence = trip.stop_times[..visit].iter().filter(|s| &s.stop == stop).count();
    network
        .patterns
        .get(&trip.pattern)?
        .stops
        .iter()
        .filter(|ps| &ps.stop == stop)
        .nth(occurrence)?
        .headsign
        .clone()
}

fn final_place(network: &Network, trip: &MaterializedTrip) -> String {
    let Some(last) = trip.stop_times.last() else { return String::new() };
    let Some(stop) = network.stops.get(&last.stop) else { return last.stop.clone() };
    stop.parent
        .as_ref()
        .and_then(|p| network.stops.get(p))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| stop.name.clone())
}
