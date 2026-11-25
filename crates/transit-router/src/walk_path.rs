
use transit_network::{Coord, Network};
use transit_streets::walking::DirectWalk;
use transit_streets::StreetPoint;

use crate::{End, Place};

pub fn walk_path(
    network: &Network,
    from_stop_id: &str,
    via_stop_ids: &[String],
    to_stop_id: &str,
    ends: [Option<&End>; 2],
    direct: Option<&DirectWalk>,
) -> Vec<Coord> {
    let place = |id: &str| -> Option<&Place> {
        ends.iter().flatten().find_map(|end| match end {
            End::Place(p) if p.id == id => Some(p),
            _ => None,
        })
    };
    let hops: Vec<&str> =
        std::iter::once(from_stop_id).chain(via_stop_ids.iter().map(String::as_str)).chain([to_stop_id]).collect();
    let coord = |p: &StreetPoint| Coord { lat: p.lat, lon: p.lon };
    let mut out: Vec<Coord> = Vec::new();
    if hops.len() == 2 && place(hops[0]).is_some() && place(hops[1]).is_some() {
        out.extend(direct.iter().flat_map(|d| d.path.iter().map(coord)));
    } else {
        for (i, hop) in hops.iter().enumerate() {
            match place(hop) {
                Some(p) => {
                    let neighbor = if i == 0 { hops[1] } else { hops[i - 1] };
                    let walk = p.walks.iter().filter(|w| w.stop_id == neighbor).min_by_key(|w| w.walk_secs);
                    let path = walk.map(|w| w.path.as_slice()).unwrap_or_default();
                    if i == 0 {
                        out.extend(path.iter().map(coord));
                    } else {
                        out.extend(path.iter().rev().map(coord));
                    }
                }
                None => out.extend(transit_network::stop_point(network, hop)),
            }
        }
    }
    out.dedup_by(|a, b| a.lat == b.lat && a.lon == b.lon);
    if out.len() < 2 {
        out.clear();
    }
    out
}
