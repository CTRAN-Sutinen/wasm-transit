//! Coverage laws over a WEIGHTED LAYER (D-47) and the coverage
//! comparison, as property tests with independent oracles.
//!
//! The decisions, restated as laws:
//!
//! * **A layer is any weighted geography** — points or polygons, its
//!   weight any numeric property (population, jobs, zero-car
//!   households), named by the planner's label. The document is the
//!   bytes: the same bytes read under another field are the same
//!   document, read another way.
//! * **Areal weighting conserves a polygon's weight**: served plus
//!   unserved is the layer's total; a polygon wholly inside a served
//!   stop's radius is wholly served, one wholly outside every radius
//!   is wholly unserved, and one straddling a radius is served by
//!   about the share of its area inside it (within the sampling
//!   resolution the answer states).
//! * **Points stay valid**: a point layer reads exactly as it always
//!   has, with its resolution (the average area per point) stated.
//! * **Frequent counts departures only (D-6)**: a stop that is only
//!   ever a trip's last stop is never frequent, on the whole day or in
//!   a window — a terminal where nobody boards is not frequent.
//! * **A combination covers what each of its dates covers (D-55,
//!   QAF-89)**: `type_coverage` and `unserved_points` answer a
//!   COMBINATION of service types (the ones that run together), pooled
//!   over its members' authored service with no date standing in — and
//!   on every date the combination holds, exactly its members run, so
//!   its served and frequent weight, its stops, and its unserved places
//!   equal the date's own coverage (the union of what runs that day).
//!   Members in any order are one combination; members that never run
//!   together answer null.
//! * **A comparison separates churn**: each side pools the service
//!   types the host names (which combination answers to which across
//!   two plans is not decided — QAF-20 — so the op never matches
//!   them); the proposal's served (and frequent) weight is the base's
//!   plus what it gains minus what it loses; a network compared with
//!   itself gains and loses nothing; swapping the sides swaps gained
//!   and lost.

use std::io::Write;

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_analysis::{coverage, coverage_compare, parse_layer, type_coverage, unserved_points, CoverageWindow};
use transit_network::service_combinations;

/// Stops sit on a line 0.01° of longitude apart at 45.5° N (about
/// 780 m), far enough that a 300 m radius around one never reaches
/// the next.
const LAT: f64 = 45.5;
fn stop_lon(i: u8) -> f64 {
    -73.5 + 0.01 * i as f64
}

#[derive(Clone, Debug)]
struct GenTrip {
    service: u8,
    start_slot: u16,
    stops: Vec<u8>,
    headway_min: u8,
    repeats: u8,
}

fn gen_trip() -> impl Strategy<Value = GenTrip> {
    (0u8..2, 0u16..60, proptest::collection::btree_set(0u8..6, 2..=4), 5u8..60, 1u8..6).prop_map(
        |(service, start_slot, stops, headway_min, repeats)| GenTrip {
            service,
            start_slot,
            stops: stops.into_iter().collect(),
            headway_min,
            repeats,
        },
    )
}

/// A feed over six stops on the line: service 0 runs weekdays,
/// service 1 weekends. Each generated trip repeats `repeats` times at
/// its headway.
fn feed(trips: &[GenTrip]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let mut add = |name: &str, body: String| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };
    add("agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n".into());
    add("routes.txt", "route_id,agency_id,route_short_name,route_type\nR,1,R,3\n".into());
    let mut stops = String::from("stop_id,stop_name,stop_lat,stop_lon");
    for i in 0..6u8 {
        stops.push_str(&format!("\ns{i},Stop {i},{LAT},{}", stop_lon(i)));
    }
    add("stops.txt", stops);
    let mut trips_txt = String::from("route_id,service_id,trip_id");
    let mut stop_times = String::from("trip_id,arrival_time,departure_time,stop_id,stop_sequence");
    let mut n = 0;
    for t in trips {
        for r in 0..t.repeats as u32 {
            let id = format!("t{n}");
            n += 1;
            trips_txt.push_str(&format!("\nR,{},{id}", t.service));
            let start = 5 * 3600 + t.start_slot as u32 * 600 + r * t.headway_min as u32 * 60;
            for (j, s) in t.stops.iter().enumerate() {
                let secs = start + j as u32 * 300;
                let hms = format!("{:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60);
                stop_times.push_str(&format!("\n{id},{hms},{hms},s{s},{}", j + 1));
            }
        }
    }
    add("trips.txt", trips_txt);
    add("stop_times.txt", stop_times);
    add(
        "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n\
         0,1,1,1,1,1,0,0,20260101,20261231\n1,0,0,0,0,0,1,1,20260101,20261231\n"
            .into(),
    );
    w.finish().unwrap().into_inner()
}

fn network(trips: &[GenTrip]) -> transit_network::Network {
    let feed = transit_codec::Feed::from_zip_bytes(&feed(trips)).expect("feed parses");
    transit_codec::import(&feed).network
}

/// A Monday.
fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, 2).unwrap()
}

/// A square of `side_m` meters centred on (lat, lon), as a GeoJSON
/// polygon ring ([lon, lat], closed).
fn square(lat: f64, lon: f64, side_m: f64) -> String {
    let dlat = side_m / 2.0 / 111_320.0;
    let dlon = side_m / 2.0 / (111_320.0 * lat.to_radians().cos());
    format!(
        "[[[{a},{b}],[{c},{b}],[{c},{d}],[{a},{d}],[{a},{b}]]]",
        a = lon - dlon,
        b = lat - dlat,
        c = lon + dlon,
        d = lat + dlat
    )
}

#[derive(Clone, Debug)]
enum Place {
    /// A point jittered near a stop (up to ~1 km away).
    Point { stop: u8, dlat: f64, dlon: f64, jobs: u32, pop: u32 },
    /// A square near a stop, `side_m` across.
    Square { stop: u8, dlat: f64, dlon: f64, side_m: f64, jobs: u32, pop: u32 },
}

fn place() -> impl Strategy<Value = Place> {
    prop_oneof![
        (0u8..6, -0.01f64..0.01, -0.01f64..0.01, 0u32..10_000, 0u32..10_000)
            .prop_map(|(stop, dlat, dlon, jobs, pop)| Place::Point { stop, dlat, dlon, jobs, pop }),
        (0u8..6, -0.008f64..0.008, -0.008f64..0.008, 20f64..2_000.0, 0u32..10_000, 0u32..10_000)
            .prop_map(|(stop, dlat, dlon, side_m, jobs, pop)| Place::Square { stop, dlat, dlon, side_m, jobs, pop }),
    ]
}

/// A layer carrying two weights per feature — `jobs` and `pop` — and a
/// string id (never a weight).
fn layer_bytes(places: &[Place]) -> Vec<u8> {
    let features: Vec<String> = places
        .iter()
        .enumerate()
        .map(|(i, p)| match p {
            Place::Point { stop, dlat, dlon, jobs, pop } => format!(
                r#"{{"type":"Feature","properties":{{"GEOID":"{i}","jobs":{jobs},"pop":{pop}}},"geometry":{{"type":"Point","coordinates":[{},{}]}}}}"#,
                stop_lon(*stop) + dlon,
                LAT + dlat
            ),
            Place::Square { stop, dlat, dlon, side_m, jobs, pop } => format!(
                r#"{{"type":"Feature","properties":{{"GEOID":"{i}","jobs":{jobs},"pop":{pop}}},"geometry":{{"type":"Polygon","coordinates":{}}}}}"#,
                square(LAT + dlat, stop_lon(*stop) + dlon, *side_m)
            ),
        })
        .collect();
    format!(r#"{{"type":"FeatureCollection","features":[{}]}}"#, features.join(",")).into_bytes()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * (1.0 + a.abs().max(b.abs()))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// Conservation: served plus unserved is the layer's total, for
    /// points and polygons alike, whichever weight is read — and the
    /// total is the sum of the features' own values.
    #[test]
    fn served_and_unserved_conserve_the_layer(
        trips in proptest::collection::vec(gen_trip(), 0..6),
        places in proptest::collection::vec(place(), 1..12),
        radius in 50f64..1_200.0,
    ) {
        let net = network(&trips);
        let layer = parse_layer(&layer_bytes(&places)).expect("layer parses");
        for field in ["jobs", "pop"] {
            let weights = layer.weights(Some(field), Some("a label")).expect("field reads");
            let expected_total: f64 = places.iter().map(|p| match p {
                Place::Point { jobs, pop, .. } | Place::Square { jobs, pop, .. } =>
                    if field == "jobs" { *jobs as f64 } else { *pop as f64 },
            }).sum();
            prop_assert!(close(weights.reading.total, expected_total), "the total is the features' sum");
            let c = coverage(&net, monday(), &weights, radius, 3600, 2, None);
            prop_assert_eq!(&c.layer.field, field);
            prop_assert_eq!(&c.layer.label, "a label");
            // Per combination: served plus its unserved places is the
            // layer's total.
            for combination in service_combinations(&net) {
                let tc = type_coverage(&net, &weights, &combination.members, radius, 3600, 2, None).expect("a combination answers");
                let unserved: f64 = unserved_points(&net, &weights, &combination.members, radius)
                    .expect("a combination answers")
                    .points
                    .iter()
                    .map(|u| u.weight)
                    .sum();
                prop_assert!(close(tc.served + unserved, weights.reading.total), "served {} + unserved {} != total {}", tc.served, unserved, weights.reading.total);
                prop_assert!(tc.frequent <= tc.served + 1e-6);
            }
        }
    }

    /// D-55 (QAF-89): a combination's coverage is what each date it
    /// holds covers. On such a date exactly its members run, so the
    /// combination's pooled served stops are the union of what runs
    /// that day: served and frequent weight, shares, stops and the
    /// unserved places all equal the date's own coverage — whole day
    /// or in a window. Members in any order are one combination; a set
    /// that never runs together answers null.
    #[test]
    fn a_combination_covers_what_each_of_its_dates_covers(
        trips in proptest::collection::vec(gen_trip(), 0..6),
        places in proptest::collection::vec(place(), 1..12),
        radius in 50f64..1_200.0,
        windowed in any::<bool>(),
    ) {
        let net = network(&trips);
        let layer = parse_layer(&layer_bytes(&places)).unwrap();
        let weights = layer.weights(Some("pop"), None).unwrap();
        let window = windowed.then_some(CoverageWindow { start_secs: 6 * 3600, end_secs: 10 * 3600 });
        let combinations = service_combinations(&net);
        for c in &combinations {
            let tc = type_coverage(&net, &weights, &c.members, radius, 1800, 2, window).expect("a combination answers");
            prop_assert_eq!(&tc.group, &c.id);
            prop_assert_eq!(&tc.members, &c.members);
            prop_assert_eq!(&tc.spans, &c.spans);
            let reversed: Vec<String> = c.members.iter().rev().cloned().collect();
            let again = type_coverage(&net, &weights, &reversed, radius, 1800, 2, window).expect("any order");
            prop_assert_eq!(again.group, tc.group.clone());
            let unserved = unserved_points(&net, &weights, &c.members, radius).expect("a combination answers");
            prop_assert_eq!(&unserved.group, &c.id);
            // Every date it holds: first, last, and a few between.
            let n = c.dates.len();
            for i in [0, n / 3, n / 2, 2 * n / 3, n - 1] {
                let date = NaiveDate::parse_from_str(&c.dates[i], "%Y-%m-%d").unwrap();
                let day = coverage(&net, date, &weights, radius, 1800, 2, window);
                prop_assert!(close(tc.served, day.served) && close(tc.frequent, day.frequent), "{} on {date}", c.id);
                prop_assert!(close(tc.served_pct, day.served_pct) && close(tc.frequent_pct, day.frequent_pct));
                prop_assert_eq!((tc.served_stops, tc.frequent_stops), (day.served_stops, day.frequent_stops));
                let unserved_day: f64 = unserved.points.iter().map(|u| u.weight).sum();
                prop_assert!(close(day.served + unserved_day, day.total), "{} on {date}: unserved is the day's complement", c.id);
            }
        }
        // The weekday and weekend services never share a day: together
        // they are no combination.
        let both = ["0".to_string(), "1".to_string()];
        prop_assert!(type_coverage(&net, &weights, &both, radius, 1800, 2, window).is_none());
        prop_assert!(unserved_points(&net, &weights, &both, radius).is_none());
        prop_assert!(type_coverage(&net, &weights, &[], radius, 1800, 2, window).is_none());
    }

    /// Monotone in the radius, polygons included.
    #[test]
    fn a_bigger_radius_never_serves_less(
        trips in proptest::collection::vec(gen_trip(), 0..6),
        places in proptest::collection::vec(place(), 1..12),
    ) {
        let net = network(&trips);
        let weights_layer = parse_layer(&layer_bytes(&places)).unwrap();
        let weights = weights_layer.weights(Some("pop"), None).unwrap();
        let mut last = (0.0, 0.0);
        for radius in [50.0, 200.0, 400.0, 800.0, 1600.0] {
            let c = coverage(&net, monday(), &weights, radius, 3600, 2, None);
            prop_assert!(c.served >= last.0 - 1e-6 && c.frequent >= last.1 - 1e-6, "radius {radius}");
            last = (c.served, c.frequent);
        }
    }

    /// D-6: a stop is frequent only on its DEPARTURES — a stop that is
    /// only ever a trip's last stop is never frequent, on the whole
    /// day or in any window, however many buses arrive there.
    #[test]
    fn a_terminal_nobody_boards_at_is_never_frequent(
        headway in 5u8..30,
        repeats in 4u8..20,
    ) {
        // Every trip runs s0 → s1; s1 is only ever arrived at.
        let net = network(&[GenTrip { service: 0, start_slot: 0, stops: vec![0, 1], headway_min: headway, repeats }]);
        let at_s1 = format!(
            r#"{{"type":"FeatureCollection","features":[{{"type":"Feature","properties":{{"w":1}},"geometry":{{"type":"Point","coordinates":[{},{LAT}]}}}}]}}"#,
            stop_lon(1)
        );
        let weights_layer = parse_layer(at_s1.as_bytes()).unwrap();
        let weights = weights_layer.weights(None, None).unwrap();
        let whole_day = coverage(&net, monday(), &weights, 100.0, 3600, 2, None);
        prop_assert!(close(whole_day.served, 1.0), "a terminal is served: riders alight there");
        prop_assert_eq!(whole_day.frequent, 0.0, "the whole-day test counts departures only");
        let window = CoverageWindow { start_secs: 5 * 3600, end_secs: 9 * 3600 };
        let windowed = coverage(&net, monday(), &weights, 100.0, 3600, 2, Some(window));
        prop_assert_eq!(windowed.frequent, 0.0, "the windowed test counts departures only");
        // The origin, where every trip departs, is frequent when its
        // headway fits.
        let at_s0 = at_s1.replace(&format!("{}", stop_lon(1)), &format!("{}", stop_lon(0)));
        let w0_layer = parse_layer(at_s0.as_bytes()).unwrap();
        let w0 = w0_layer.weights(None, None).unwrap();
        let origin = coverage(&net, monday(), &w0, 100.0, 3600, 2, None);
        prop_assert!(close(origin.frequent, 1.0), "the origin departs every {headway} min");
    }

    /// The comparison separates churn: each side pools the service
    /// types the host names (no date, no matching between the plans —
    /// QAF-20), and the proposal's served (and frequent) weight is the
    /// base's plus what it gains minus what it loses.
    #[test]
    fn a_comparison_is_gains_and_losses(
        base_trips in proptest::collection::vec(gen_trip(), 0..6),
        proposal_trips in proptest::collection::vec(gen_trip(), 0..6),
        places in proptest::collection::vec(place(), 1..12),
        radius in 100f64..900.0,
        base_members in proptest::sample::subsequence(vec!["0".to_string(), "1".to_string()], 0..=2),
        members in proptest::sample::subsequence(vec!["0".to_string(), "1".to_string()], 0..=2),
    ) {
        let base = network(&base_trips);
        let proposal = network(&proposal_trips);
        let layer = parse_layer(&layer_bytes(&places)).unwrap();
        let weights = layer.weights(Some("jobs"), Some("jobs")).unwrap();
        let ab = coverage_compare(&base, &proposal, &weights, &base_members, &members, radius, 1800, 2, None);
        let ba = coverage_compare(&proposal, &base, &weights, &members, &base_members, radius, 1800, 2, None);
        let aa = coverage_compare(&base, &base, &weights, &base_members, &base_members, radius, 1800, 2, None);
        prop_assert!(close(ab.proposal.served, ab.base.served + ab.served_gained - ab.served_lost));
        prop_assert!(close(ab.proposal.frequent, ab.base.frequent + ab.frequent_gained - ab.frequent_lost));
        prop_assert!(ab.served_gained >= 0.0 && ab.served_lost >= 0.0);
        prop_assert!(close(ab.served_gained, ba.served_lost) && close(ab.served_lost, ba.served_gained));
        prop_assert!(close(ab.frequent_gained, ba.frequent_lost) && close(ab.frequent_lost, ba.frequent_gained));
        prop_assert_eq!(aa.served_gained + aa.served_lost + aa.frequent_gained + aa.frequent_lost, 0.0);
        prop_assert_eq!(&ab.base.members, &base_members);
        // A side of the weekday type alone is the weekday's coverage
        // (only that type runs on a Monday).
        if base_members == ["0"] {
            let on_monday = coverage(&base, monday(), &weights, radius, 1800, 2, None);
            prop_assert!(close(ab.base.served, on_monday.served) && close(ab.base.frequent, on_monday.frequent));
        }
    }
}

/// A polygon wholly inside a served stop's radius is wholly served;
/// one wholly beyond every radius is wholly unserved; one straddling
/// a radius is served by about the share of its area inside it.
#[test]
fn areal_weighting_serves_the_share_of_the_area() {
    // s0 → s5: the served stops are 3.9 km apart, so no radius here
    // reaches from one to the other's square.
    let net = network(&[GenTrip { service: 0, start_slot: 0, stops: vec![0, 5], headway_min: 10, repeats: 6 }]);
    let layer = format!(
        r#"{{"type":"FeatureCollection","features":[
            {{"type":"Feature","properties":{{"jobs":100}},"geometry":{{"type":"Polygon","coordinates":{inside}}}}},
            {{"type":"Feature","properties":{{"jobs":40}},"geometry":{{"type":"Polygon","coordinates":{outside}}}}},
            {{"type":"Feature","properties":{{"jobs":1000}},"geometry":{{"type":"Polygon","coordinates":{straddle}}}}}
        ]}}"#,
        // 200 m across, centred on s0: its corners are 141 m out.
        inside = square(LAT, stop_lon(0), 200.0),
        // 200 m across, 5 km north of the line.
        outside = square(LAT + 0.045, stop_lon(0), 200.0),
        // 2 km across, centred on s5: a 400 m radius covers π·400² of
        // its 2000² m² — 12.57%.
        straddle = square(LAT, stop_lon(5), 2_000.0),
    );
    let weights_layer = parse_layer(layer.as_bytes()).unwrap();
        let weights = weights_layer.weights(Some("jobs"), Some("jobs")).unwrap();
    assert_eq!(weights.reading.polygons, 3);
    assert_eq!(weights.reading.points, 0);
    let spacing = weights.reading.sample_spacing_m.expect("a polygon layer states its sampling");
    assert!(spacing > 0.0 && spacing <= 40.0, "the 2 km square samples at {spacing} m");

    let unserved = unserved_points(&net, &weights, &["0".to_string()], 400.0).expect("the weekday combination").points;
    let by_weight = |w: f64| unserved.iter().find(|u| (u.weight - w).abs() < 1e-6);
    assert!(unserved.iter().all(|u| u.feature != 0), "the inside square is wholly served: {unserved:?}");
    let out = by_weight(40.0).expect("the far square is wholly unserved");
    assert!((out.share - 1.0).abs() < 1e-9 && out.polygon.is_some());
    let straddle = unserved.iter().find(|u| u.feature == 2).expect("the big square is partly unserved");
    let served_share = 1.0 - straddle.share;
    let expected = std::f64::consts::PI * 400.0 * 400.0 / (2_000.0 * 2_000.0);
    assert!((served_share - expected).abs() < 0.01, "served share {served_share} ≈ {expected}");

    let c = coverage(&net, monday(), &weights, 400.0, 3600, 2, None);
    assert!((c.served - (100.0 + 1000.0 * served_share)).abs() < 1e-6);
}

/// Points stay valid: a point is served whole or not at all, and the
/// answer states the layer's resolution as the average area per point.
#[test]
fn a_point_layer_states_its_resolution() {
    let layer = format!(
        r#"{{"type":"FeatureCollection","features":[
            {{"type":"Feature","properties":{{"POP":10,"GEOID":"1"}},"geometry":{{"type":"Point","coordinates":[{a},{LAT}]}}}},
            {{"type":"Feature","properties":{{"POP":20,"GEOID":"2"}},"geometry":{{"type":"Point","coordinates":[{b},{c}]}}}}
        ]}}"#,
        a = stop_lon(0),
        b = stop_lon(0) + 0.02,
        c = LAT + 0.02,
    );
    let layer = parse_layer(layer.as_bytes()).unwrap();
    // One numeric property: it is the weight when none is named; the
    // string id never is.
    let weights = layer.weights(None, None).unwrap();
    assert_eq!(weights.reading.field, "POP");
    assert_eq!(weights.reading.label, "POP", "the label defaults to the field");
    assert_eq!((weights.reading.points, weights.reading.polygons), (2, 0));
    let spacing = weights.reading.point_spacing_m.expect("a point layer states its spacing");
    assert!(spacing > 1_000.0 && spacing < 2_500.0, "two points ~2 km apart: {spacing}");
    assert!(layer.weights(Some("jobs"), None).is_err(), "a field the layer lacks is an error");
}

/// The document is the bytes: two weights read from one file are one
/// layer, and a layer of several numeric fields names none by default.
#[test]
fn a_layer_with_several_weights_needs_one_named() {
    let bytes = layer_bytes(&[Place::Point { stop: 0, dlat: 0.0, dlon: 0.0, jobs: 5, pop: 7 }]);
    let layer = parse_layer(&bytes).unwrap();
    let names: Vec<&str> = layer.fields().iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["jobs", "pop"], "numeric properties only (GEOID is a string)");
    let err = layer.weights(None, None).err().expect("two weights: name one");
    assert!(err.contains("jobs") && err.contains("pop"), "{err}");
    assert_eq!(layer.weights(Some("jobs"), None).unwrap().reading.total, 5.0);
    assert_eq!(layer.weights(Some("pop"), None).unwrap().reading.total, 7.0);
}

/// type_coverage reads the weighted layer the same way.
#[test]
fn type_coverage_reads_any_weight() {
    let net = network(&[GenTrip { service: 0, start_slot: 0, stops: vec![0, 1], headway_min: 10, repeats: 6 }]);
    let bytes = layer_bytes(&[
        Place::Square { stop: 0, dlat: 0.0, dlon: 0.0, side_m: 100.0, jobs: 30, pop: 10 },
        Place::Point { stop: 3, dlat: 0.0, dlon: 0.0, jobs: 70, pop: 90 },
    ]);
    let layer = parse_layer(&bytes).unwrap();
    let weekday = ["0".to_string()];
    let jobs = type_coverage(&net, &layer.weights(Some("jobs"), None).unwrap(), &weekday, 300.0, 3600, 2, None).unwrap();
    let pop = type_coverage(&net, &layer.weights(Some("pop"), None).unwrap(), &weekday, 300.0, 3600, 2, None).unwrap();
    assert!((jobs.served_pct - 30.0).abs() < 1e-9, "{jobs:?}");
    assert!((pop.served_pct - 10.0).abs() < 1e-9, "{pop:?}");
}
