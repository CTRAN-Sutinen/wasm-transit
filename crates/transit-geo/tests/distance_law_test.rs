//! The laws of the one distance (D-250): a great-circle distance on a
//! sphere of the mean earth radius, and the local frame that agrees
//! with it at street scale.

use proptest::prelude::*;
use transit_geo::{distance_m, meters_per_degree_lon, EARTH_RADIUS_M, METERS_PER_DEGREE};

fn lat() -> impl Strategy<Value = f64> {
    -89.9f64..89.9
}

fn lon() -> impl Strategy<Value = f64> {
    -180.0f64..180.0
}

/// A point near (lat, lon): within about 5 km.
fn near() -> impl Strategy<Value = (f64, f64)> {
    (-0.045f64..0.045, -0.045f64..0.045)
}

#[test]
fn one_degree_of_latitude_is_the_mean_radius_arc() {
    assert!((METERS_PER_DEGREE - 111_194.926_6).abs() < 1e-3, "{METERS_PER_DEGREE}");
    assert!((distance_m(45.0, -122.0, 46.0, -122.0) - METERS_PER_DEGREE).abs() < 1e-6);
}

/// Near the antipodes, `asin` near 1 loses precision: a few decimeters
/// in 20,000 km.
#[test]
fn antipodes_are_half_the_circumference() {
    let d = distance_m(10.0, 20.0, -10.0, -160.0);
    assert!(d.is_finite());
    assert!((d - EARTH_RADIUS_M * std::f64::consts::PI).abs() < 1.0, "{d}");
}

#[test]
fn the_antimeridian_is_no_wall() {
    let across = distance_m(0.0, 179.99, 0.0, -179.99);
    assert!((across - 0.02 * METERS_PER_DEGREE).abs() < 1e-3, "{across}");
}

proptest! {
    #[test]
    fn a_point_is_zero_from_itself(a in lat(), b in lon()) {
        prop_assert_eq!(distance_m(a, b, a, b), 0.0);
    }

    #[test]
    fn distance_is_symmetric(a in lat(), b in lon(), c in lat(), d in lon()) {
        prop_assert_eq!(distance_m(a, b, c, d), distance_m(c, d, a, b));
    }

    #[test]
    fn distance_is_between_zero_and_half_the_circumference(a in lat(), b in lon(), c in lat(), d in lon()) {
        let m = distance_m(a, b, c, d);
        prop_assert!(m >= 0.0);
        prop_assert!(m <= EARTH_RADIUS_M * std::f64::consts::PI + 1e-6);
    }

    #[test]
    fn no_detour_is_shorter(a in lat(), b in lon(), c in lat(), d in lon(), e in lat(), f in lon()) {
        let direct = distance_m(a, b, e, f);
        let via = distance_m(a, b, c, d) + distance_m(c, d, e, f);
        prop_assert!(direct <= via + 1e-6, "{direct} > {via}");
    }

    #[test]
    fn a_meridian_is_its_latitude_difference(a in lat(), c in lat(), b in lon()) {
        let m = distance_m(a, b, c, b);
        prop_assert!((m - (a - c).abs() * METERS_PER_DEGREE).abs() < 1e-6 * m.max(1.0), "{m}");
    }

    #[test]
    fn the_equator_is_its_longitude_difference(b in lon(), d in lon()) {
        let span = (b - d).abs().min(360.0 - (b - d).abs());
        let m = distance_m(0.0, b, 0.0, d);
        prop_assert!((m - span * METERS_PER_DEGREE).abs() < 1e-6 * m.max(1.0), "{m}");
    }

    /// The local frame (meters per degree of latitude, and of
    /// longitude at a latitude) agrees with the distance to 0.1% within
    /// 5 km, away from the poles.
    #[test]
    fn the_local_frame_agrees_at_street_scale(a in -70.0f64..70.0, b in lon(), (dlat, dlon) in near()) {
        let (c, d) = (a + dlat, b + dlon);
        let mid = (a + c) / 2.0;
        let flat = (dlat * METERS_PER_DEGREE).hypot(dlon * meters_per_degree_lon(mid));
        let round = distance_m(a, b, c, d);
        prop_assert!((flat - round).abs() <= 1e-3 * round + 1e-6, "flat {flat}, round {round}");
    }

    #[test]
    fn a_degree_of_longitude_shrinks_with_the_cosine(a in lat()) {
        let m = meters_per_degree_lon(a);
        prop_assert!((m - METERS_PER_DEGREE * a.to_radians().cos()).abs() < 1e-9);
        prop_assert!(m > 0.0 && m <= METERS_PER_DEGREE);
    }
}
