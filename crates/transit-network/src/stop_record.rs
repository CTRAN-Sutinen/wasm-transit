//! A stops document begun from a network's stops (`transit-stops`;
//! D-153, D-157). A pick only reads the stops document; [`seed`] is how
//! the first one is made, from the stops a network already records.

use std::collections::BTreeMap;

use transit_stops::{Details, Event, EventKind, Stops};

use crate::places::StopKind;
use crate::{Network, Stop};

/// A stops document from a network's stops: each stop one `in_service`
/// event with no date (no feed records when a stop went into service),
/// carrying its point and every detail.
pub fn seed(network: &Network) -> Stops {
    let stops = network.stops.iter().map(|(id, stop)| (id.clone(), vec![opening(stop)])).collect();
    Stops { stops, ..Stops::default() }
}

fn opening(stop: &Stop) -> Event {
    let info = &stop.info;
    Event {
        event: EventKind::InService,
        lat: Some(stop.lat),
        lon: Some(stop.lon),
        details: Details {
            name: Some(stop.name.clone()).filter(|n| !n.is_empty()),
            kind: (stop.kind == StopKind::Station).then_some(transit_stops::StopKind::Station),
            parent: stop.parent.clone(),
            code: info.code.clone(),
            spoken_name: info.spoken_name.clone(),
            description: info.description.clone(),
            url: info.url.clone(),
            fare_zone: info.fare_zone.clone(),
            wheelchair: info.wheelchair,
            transfer: stop.transfer.then_some(true),
            platform: info.platform.clone(),
            level: info.level.clone(),
            timezone: info.timezone.clone(),
            street_access: info.street_access,
            amenities: BTreeMap::new(),
        },
        ..Event::default()
    }
}
