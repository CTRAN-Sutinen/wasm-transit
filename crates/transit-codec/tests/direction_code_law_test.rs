//! Model directions and GTFS's binary `direction_id`. The rule is on
//! `gtfs_direction`.

use proptest::prelude::*;
use std::collections::BTreeMap;
use transit_codec::{gtfs_direction, model_direction};
use transit_network::{Direction, Route};

fn route(ids: &[&str]) -> Route {
    let directions: BTreeMap<String, Direction> = ids
        .iter()
        .map(|id| {
            (
                id.to_string(),
                Direction {
                    name: format!("To {id}"),
                    extra: Default::default(),
                },
            )
        })
        .collect();
    Route {
        directions,
        ..Default::default()
    }
}

#[test]
fn literal_codes_are_themselves() {
    let r = route(&["0", "1"]);
    assert_eq!(gtfs_direction(&r, "0"), Some(0));
    assert_eq!(gtfs_direction(&r, "1"), Some(1));
    assert_eq!(model_direction(&r, 0).map(String::as_str), Some("0"));
    assert_eq!(model_direction(&r, 1).map(String::as_str), Some("1"));
}

#[test]
fn other_ids_take_the_free_codes_in_id_order() {
    let r = route(&["in", "out"]);
    assert_eq!(gtfs_direction(&r, "in"), Some(0));
    assert_eq!(gtfs_direction(&r, "out"), Some(1));
    let r = route(&["0", "x"]);
    assert_eq!(gtfs_direction(&r, "x"), Some(1));
    assert_eq!(model_direction(&r, 1).map(String::as_str), Some("x"));
}

#[test]
fn a_third_direction_has_no_code_and_an_unknown_code_names_nothing() {
    let r = route(&["a", "b", "c"]);
    assert_eq!(gtfs_direction(&r, "c"), None);
    assert_eq!(model_direction(&r, 2), None);
    assert_eq!(gtfs_direction(&r, "missing"), None);
    assert_eq!(model_direction(&route(&[]), 0), None);
}

proptest! {
    /// Every code that the export writes maps back to its own direction, and
    /// no two directions of a route share a code.
    #[test]
    fn the_reader_inverts_the_export(ids in prop::collection::btree_set("[0-3a-c]{1,2}", 0..5)) {
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        let r = route(&ids);
        let mut seen = BTreeMap::new();
        for id in &ids {
            if let Some(code) = gtfs_direction(&r, id) {
                prop_assert!(code <= 1);
                prop_assert_eq!(model_direction(&r, u32::from(code)).map(String::as_str), Some(*id));
                prop_assert!(seen.insert(code, *id).is_none(), "two directions share code {}", code);
            }
        }
        for code in 0..3u32 {
            if let Some(id) = model_direction(&r, code) {
                prop_assert_eq!(gtfs_direction(&r, id).map(u32::from), Some(code));
            }
        }
    }
}
