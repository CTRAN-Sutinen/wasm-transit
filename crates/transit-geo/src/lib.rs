//! The one distance between two coordinates (D-250;
//! `docs/architecture/crates/transit-geo.md`).
//!
//! - [`distance_m`] is the great-circle distance in meters on a sphere
//!   of the mean earth radius (haversine). Every library that reports a
//!   distance or compares one with a radius uses it, so two answers
//!   about the same two points agree.
//! - [`METERS_PER_DEGREE`] and [`meters_per_degree_lon`] are the local
//!   frame on the same sphere: for code that must work in flat
//!   coordinates (a grid, a projection onto a segment, an area). Within
//!   5 km, the frame agrees with [`distance_m`] to 0.1%.
//!
//! The crate depends on no other crate, so any library can use it.

/// The mean earth radius in meters.
pub const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Meters in one degree of latitude, on the same sphere.
pub const METERS_PER_DEGREE: f64 = EARTH_RADIUS_M * std::f64::consts::PI / 180.0;

/// The great-circle distance in meters between two coordinates in
/// degrees (haversine).
pub fn distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    // Rounding can put `a` just above 1 at the antipodes.
    2.0 * EARTH_RADIUS_M * a.sqrt().min(1.0).asin()
}

/// Meters in one degree of longitude at a latitude in degrees.
pub fn meters_per_degree_lon(lat: f64) -> f64 {
    METERS_PER_DEGREE * lat.to_radians().cos()
}
