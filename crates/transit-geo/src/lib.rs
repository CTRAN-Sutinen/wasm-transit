//! One distance for every library: the great-circle distance on a sphere of
//! the mean earth radius (6,371,000 m), so two libraries never disagree on how
//! far two points are. `distance_m` ends in `2·asin(√a)`, as the libraries
//! that used it before wrote it, so their answers keep the same bytes. Within
//! 5 km the local frame agrees with it to 0.1%.

pub const EARTH_RADIUS_M: f64 = 6_371_000.0;

pub const METERS_PER_DEGREE: f64 = EARTH_RADIUS_M * std::f64::consts::PI / 180.0;

pub fn distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    // Rounding can put `a` just above 1 at the antipodes.
    2.0 * EARTH_RADIUS_M * a.sqrt().min(1.0).asin()
}

pub fn meters_per_degree_lon(lat: f64) -> f64 {
    METERS_PER_DEGREE * lat.to_radians().cos()
}
