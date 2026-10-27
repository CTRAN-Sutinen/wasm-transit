//! The network's geometry answers: a pattern draws its authored path,
//! else its stops joined; an entity frames every pattern it runs.

use transit_network::{entity_focus, pattern_alignment, Coord, EntityKind, Network};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap()
}

fn c(lat: f64, lon: f64) -> Coord {
    Coord { lat, lon }
}

#[test]
fn a_pattern_draws_its_path_else_its_stops() {
    let mut net = network();
    assert_eq!(pattern_alignment(&net, "10-inbound"), vec![c(45.5, -73.5), c(45.51, -73.51), c(45.52, -73.52)]);
    let path = vec![c(45.5, -73.5), c(45.505, -73.5), c(45.52, -73.52)];
    net.patterns.get_mut("10-inbound").unwrap().path = Some(path.clone());
    assert_eq!(pattern_alignment(&net, "10-inbound"), path);
    net.patterns.get_mut("10-inbound").unwrap().path = Some(vec![c(1.0, 1.0)]);
    assert_eq!(pattern_alignment(&net, "10-inbound").len(), 3, "a one-point path is no path");
    assert!(pattern_alignment(&net, "nope").is_empty());
}

#[test]
fn an_entity_frames_what_it_runs() {
    let net = network();
    for (kind, id) in [
        (EntityKind::Pattern, "10-inbound"),
        (EntityKind::Schedule, "10-weekday"),
        (EntityKind::Route, "10"),
        (EntityKind::ServiceType, "weekday"),
        (EntityKind::Block, "400"),
    ] {
        let focus = entity_focus(&net, kind, id).unwrap_or_else(|| panic!("{kind:?} {id}"));
        assert_eq!((focus.lines.len(), focus.points.len()), (1, 3), "{kind:?}");
    }
    let stop = entity_focus(&net, EntityKind::Stop, "B").unwrap();
    assert_eq!((stop.lines.len(), stop.points), (0, vec![c(45.51, -73.51)]));
    for kind in [EntityKind::Stop, EntityKind::Route, EntityKind::Pattern, EntityKind::Schedule, EntityKind::ServiceType, EntityKind::Block] {
        assert!(entity_focus(&net, kind, "nope").is_none(), "{kind:?}");
    }
}
