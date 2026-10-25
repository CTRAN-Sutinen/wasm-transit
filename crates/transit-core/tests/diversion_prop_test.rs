//! Diversion patching laws, as property tests with independent oracles.
//!
//! `diversion_test.rs` covers scenarios by hand; the one property in
//! `prop_test.rs` builds 2-stop trips, where `skipped_stops` is always
//! a no-op. Here every trip has an interior stop, so skip semantics
//! are actually observable, and the laws must hold for ANY valid
//! diversion the generator can produce:
//!
//! * **Identity** — no diversion in force on the date (empty list, or
//!   windows that don't cover it) ⇒ patched rows are the base rows,
//!   sequences included.
//! * **Immutability** — the base feed survives any apply unchanged.
//! * **Commutativity on disjoint scopes** — diversions whose affected
//!   trip sets cannot overlap (different `route_id`s) commute:
//!   permuting the list yields identical patched rows. Order matters
//!   *by design* when scopes overlap, so disjointness is the law.
//! * **Oracle** — one diversion: every trip is predictable from the
//!   spec. Matching trips (route, direction when named, anchors in
//!   order) lose their interior skipped stop, renumber from 0, and
//!   shift from the last anchor onward; every other trip is untouched
//!   with its original sequences.

use chrono::{Duration, NaiveDate};
use proptest::collection::vec;
use proptest::prelude::*;
use std::io::Write;
use transit_core::{fmt_time, Gtfs};

const EPOCH: NaiveDate = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
/// A Wednesday — irrelevant to diversions (no calendar filtering
/// here), just a fixed query date.
fn query_date() -> NaiveDate {
    EPOCH + Duration::days(160)
}

fn date_2026(day: i64) -> NaiveDate {
    EPOCH + Duration::days(day)
}

// ---------------------------------------------------------------------
// Generator: feeds whose trips have an interior stop
// ---------------------------------------------------------------------

#[derive(Clone, Debug)]
struct TripSpec {
    route: usize,
    dir: u8,
    start: u32,
    seg1: u32,
    seg2: u32,
    first: usize,
    mid: usize,
    last: usize,
}

impl TripSpec {
    fn stop(&self, i: usize) -> String {
        format!("s{i}")
    }

    /// The rows the zip writes for this trip, from the spec alone.
    fn base_rows(&self) -> Vec<(String, u32, Option<u32>, Option<u32>)> {
        let t0 = self.start;
        let t1 = self.start + self.seg1;
        let t2 = t1 + self.seg2;
        vec![
            (self.stop(self.first), 1, Some(t0), Some(t0)),
            (self.stop(self.mid), 2, Some(t1), Some(t1)),
            (self.stop(self.last), 3, Some(t2), Some(t2)),
        ]
    }
}

#[derive(Clone, Debug)]
struct FeedSpec {
    n_routes: usize,
    n_stops: usize,
    trips: Vec<TripSpec>,
}

fn trip_spec(n_routes: usize, n_stops: usize) -> impl Strategy<Value = TripSpec> {
    // Distinct stops by arithmetic: mid is a's offset, last is picked
    // from the remaining indices — no filtering, no assume.
    (
        0usize..n_routes,
        0u8..=1,
        0u32..90_000,
        0u32..3_600,
        0u32..3_600,
        0usize..n_stops,
        1usize..n_stops,
        1usize..n_stops - 1,
    )
        .prop_map(move |(route, dir, start, seg1, seg2, first, off1, off2)| {
            let mid = (first + off1) % n_stops;
            let rest: Vec<usize> = (0..n_stops).filter(|i| *i != first && *i != mid).collect();
            let last = rest[(off2 - 1) % rest.len()];
            TripSpec { route, dir, start, seg1, seg2, first, mid, last }
        })
}

fn feed_spec(min_routes: usize, min_trips: usize) -> impl Strategy<Value = FeedSpec> {
    (min_routes..=3, 3usize..=6).prop_flat_map(move |(n_routes, n_stops)| {
        vec(trip_spec(n_routes, n_stops), min_trips..=8).prop_map(move |trips| FeedSpec {
            n_routes,
            n_stops,
            trips: trips.clone(),
        })
    })
}

fn build_zip(spec: &FeedSpec) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };

    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");

    let mut s = String::from("route_id,agency_id,route_short_name,route_long_name,route_type\n");
    for r in 0..spec.n_routes {
        s.push_str(&format!("r{r},1,r{r},,3\n"));
    }
    add(&mut w, "routes.txt", &s);

    let mut s = String::from("stop_id,stop_code,stop_name,stop_lat,stop_lon,location_type,parent_station\n");
    for i in 0..spec.n_stops {
        s.push_str(&format!("s{i},,Stop {i},0.5,0.5,0,\n"));
    }
    add(&mut w, "stops.txt", &s);

    // One all-year, all-days service: diversions don't filter by
    // calendar, so this only needs to parse.
    add(&mut w, "calendar.txt", "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nsvc0,1,1,1,1,1,1,1,20260101,20261231\n");

    let mut s = String::from("route_id,service_id,trip_id,trip_headsign,direction_id,block_id\n");
    for (i, t) in spec.trips.iter().enumerate() {
        s.push_str(&format!("r{},svc0,trip{i},,{},\n", t.route, t.dir));
    }
    add(&mut w, "trips.txt", &s);

    let mut s = String::from("trip_id,arrival_time,departure_time,stop_id,stop_sequence,stop_headsign\n");
    for (i, t) in spec.trips.iter().enumerate() {
        for (stop, seq, time) in [
            (t.stop(t.first), 1, t.start),
            (t.stop(t.mid), 2, t.start + t.seg1),
            (t.stop(t.last), 3, t.start + t.seg1 + t.seg2),
        ] {
            s.push_str(&format!("trip{i},{},{},{},{seq},\n", fmt_time(time), fmt_time(time), stop));
        }
    }
    add(&mut w, "stop_times.txt", &s);

    w.finish().expect("finish zip").into_inner()
}

/// One trip's rows as tuples, via the public query API — sorted by
/// `stop_sequence`, so comparisons are independent of the Vec order
/// `apply_diversions` reassembles from its HashMap.
fn rows(gtfs: &Gtfs, trip_id: &str) -> Vec<(String, u32, Option<u32>, Option<u32>)> {
    gtfs.trip_stop_times(trip_id)
        .expect("trip exists")
        .into_iter()
        .map(|r| (r.stop_id, r.stop_sequence, r.arrival_time, r.departure_time))
        .collect()
}

fn all_rows(gtfs: &Gtfs, spec: &FeedSpec) -> Vec<Vec<(String, u32, Option<u32>, Option<u32>)>> {
    (0..spec.trips.len())
        .map(|i| rows(gtfs, &format!("trip{i}")))
        .collect()
}

// ---------------------------------------------------------------------
// Diversion generator
// ---------------------------------------------------------------------

/// How the diversion names the direction: not at all, the trip's own
/// direction (matches), or the other one (cannot match).
#[derive(Clone, Copy, Debug)]
enum DirPick {
    None,
    Own,
    Other,
}

#[derive(Debug)]
struct DivSpec {
    trip: usize,
    dir_pick: DirPick,
    skip_mid: bool,
    added: u32,
    detour: Option<Vec<(f64, f64)>>,
    start_day: i64,
    end_day: i64,
}

/// Day index of the fixed query date (see `query_date`).
const QUERY_DAY: i64 = 160;

/// An arbitrary window: usually near the query date, covering it only
/// by chance. Yields absolute day indices with start <= end.
fn window_any() -> impl Strategy<Value = (i64, i64)> {
    (150i64..170, 0i64..50).prop_map(|(start, len)| (start, start + len))
}

/// A window that covers the query date by construction — assuming it
/// instead starves proptest of cases.
fn window_covering() -> impl Strategy<Value = (i64, i64)> {
    (-10i64..=0, 0i64..=10).prop_map(|(start_off, end_off)| (QUERY_DAY + start_off, QUERY_DAY + end_off))
}

fn div_spec<W: Strategy<Value = (i64, i64)>>(
    n_trips: usize,
    window: W,
) -> impl Strategy<Value = DivSpec> {
    (
        0usize..n_trips,
        0u8..3,
        any::<bool>(),
        0u32..600,
        proptest::option::of(vec((0.0f64..1.0, 0.0f64..1.0), 2..=4)),
        window,
    )
        .prop_map(move |(trip, dir_pick, skip_mid, added, detour, (start_day, end_day))| DivSpec {
            trip,
            dir_pick: match dir_pick {
                0 => DirPick::None,
                1 => DirPick::Own,
                _ => DirPick::Other,
            },
            skip_mid,
            added,
            detour,
            start_day,
            end_day,
        })
}

use transit_core::{apply_diversions, validate_diversion, DetourPoint, Diversion};

fn make_diversion(spec: &FeedSpec, d: &DivSpec) -> Diversion {
    let t = &spec.trips[d.trip];
    Diversion {
        id: "d".into(),
        route_id: format!("r{}", t.route),
        direction_id: match d.dir_pick {
            DirPick::None => None,
            DirPick::Own => Some(t.dir),
            DirPick::Other => Some(1 - t.dir),
        },
        first_kept_stop: t.stop(t.first),
        last_kept_stop: t.stop(t.last),
        skipped_stops: if d.skip_mid { vec![t.stop(t.mid)] } else { vec![] },
        detour: d
            .detour
            .as_ref()
            .map(|pts| pts.iter().map(|&(lat, lon)| DetourPoint { lat, lon }).collect()),
        added_secs: d.added,
        start_date: date_2026(d.start_day),
        end_date: date_2026(d.end_day),
    }
}

/// The direction the generated diversion names, from the spec alone.
fn oracle_direction(spec: &FeedSpec, d: &DivSpec) -> Option<u8> {
    let anchor = &spec.trips[d.trip];
    match d.dir_pick {
        DirPick::None => None,
        DirPick::Own => Some(anchor.dir),
        DirPick::Other => Some(1 - anchor.dir),
    }
}

/// The trips this diversion can affect, computed from the spec with
/// none of the implementation's code: same route, the named direction
/// when there is one, and both anchors present in order — anywhere in
/// the trip, not only at its endpoints.
fn oracle_matches(spec: &FeedSpec, d: &DivSpec, trip: usize) -> bool {
    let anchor = &spec.trips[d.trip];
    let t = &spec.trips[trip];
    if t.route != anchor.route {
        return false;
    }
    if let Some(dir) = oracle_direction(spec, d) {
        if t.dir != dir {
            return false;
        }
    }
    let stops = [t.first, t.mid, t.last];
    let p0 = stops.iter().position(|s| *s == anchor.first);
    p0.and_then(|p0| {
        stops[p0 + 1..]
            .iter()
            .position(|s| *s == anchor.last)
            .map(|_| true)
    })
    .is_some()
}

/// Expected rows for one trip after the diversion is applied on a day
/// its window covers, from the spec alone: interior rows between the
/// anchors at the skipped stop go away, sequences renumber from 0,
/// and everything from the last anchor onward shifts by added_secs.
fn oracle_rows(spec: &FeedSpec, d: &DivSpec, trip: usize) -> Vec<(String, u32, Option<u32>, Option<u32>)> {
    let t = &spec.trips[trip];
    let base = t.base_rows();
    if !oracle_matches(spec, d, trip) {
        return base;
    }
    let anchor = &spec.trips[d.trip];
    let stops = [t.first, t.mid, t.last];
    let p0 = stops
        .iter()
        .position(|s| *s == anchor.first)
        .expect("oracle_matches guarantees the first anchor");
    let p1 = p0
        + stops[p0 + 1..]
            .iter()
            .position(|s| *s == anchor.last)
            .expect("oracle_matches guarantees the last anchor")
        + 1;
    let skipped = if d.skip_mid { Some(anchor.stop(anchor.mid)) } else { None };

    let mut kept: Vec<(String, u32, Option<u32>, Option<u32>)> = Vec::new();
    let mut last_kept_pos = 0;
    for (i, row) in base.into_iter().enumerate() {
        let interior = i > p0 && i < p1;
        if interior && Some(&row.0) == skipped.as_ref() {
            continue;
        }
        if i == p1 {
            last_kept_pos = kept.len();
        }
        kept.push(row);
    }
    // Renumber from 0, contiguous — matching the documented contract.
    for (i, row) in kept.iter_mut().enumerate() {
        row.1 = i as u32;
    }
    for row in &mut kept[last_kept_pos..] {
        if let Some(time) = row.2 {
            row.2 = Some(time + d.added);
        }
        if let Some(time) = row.3 {
            row.3 = Some(time + d.added);
        }
    }
    kept
}

// ---------------------------------------------------------------------
// Laws
// ---------------------------------------------------------------------

/// A feed spec paired with a diversion spec legal for it, window
/// arbitrary (may or may not be in force on the query date).
fn spec_and_div() -> impl Strategy<Value = (FeedSpec, DivSpec)> {
    feed_spec(1, 1).prop_flat_map(|spec| {
        let n = spec.trips.len();
        (Just(spec), div_spec(n, window_any()))
    })
}

/// A feed spec paired with a diversion whose window covers the query
/// date by construction — assuming coverage starves proptest.
fn spec_and_div_in_force() -> impl Strategy<Value = (FeedSpec, DivSpec)> {
    feed_spec(1, 1).prop_flat_map(|spec| {
        let n = spec.trips.len();
        (Just(spec), div_spec(n, window_covering()))
    })
}

/// A feed spec (>= 2 routes, >= 2 trips) paired with two diversion
/// specs in force on the query date and anchored on trips of
/// different routes — disjoint scopes by construction, no rejection.
fn spec_and_disjoint_pair() -> impl Strategy<Value = (FeedSpec, DivSpec, DivSpec)> {
    feed_spec(2, 2).prop_flat_map(|spec| {
        let n = spec.trips.len();
        (Just(spec), div_spec(n, window_covering()), div_spec(n, window_covering())).prop_map(
            move |(mut spec, mut a, mut b)| {
                // Deterministic re-anchoring: trip 0 and trip 1, with
                // trip 1 forced onto a different route (the spec has
                // >= 2 routes, so one exists).
                let r0 = spec.trips[0].route;
                let r1 = (r0 + 1) % spec.n_routes;
                spec.trips[1].route = r1;
                a.trip = 0;
                b.trip = 1;
                assert_ne!(spec.trips[a.trip].route, spec.trips[b.trip].route);
                (spec, a, b)
            },
        )
    })
}

/// Does [start_day, end_day] cover query_date()?
fn covers_query(d: &DivSpec) -> bool {
    date_2026(d.start_day) <= query_date() && query_date() <= date_2026(d.end_day)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Identity: with no diversion in force on the date — an empty
    /// list, or a valid diversion whose window misses the date — the
    /// patched feed's rows are the base feed's rows, sequences and
    /// all. The base feed is likewise unchanged.
    #[test]
    fn no_diversion_in_force_is_identity(
        (spec, d_raw) in spec_and_div(),
    ) {
        // Window misses the date: the diversion is a no-op, not an
        // error, whatever its content.
        prop_assume!(!covers_query(&d_raw));
        let base = Gtfs::from_zip_bytes(&build_zip(&spec)).expect("parse");
        let before = all_rows(&base, &spec);

        let empty = apply_diversions(&base, query_date(), &[]).expect("apply");
        prop_assert_eq!(all_rows(&empty, &spec), before.clone());

        let d = make_diversion(&spec, &d_raw);
        validate_diversion(&base, &d).expect("generated diversion is valid");
        let patched = apply_diversions(&base, query_date(), &[d]).expect("apply");
        prop_assert_eq!(all_rows(&patched, &spec), before.clone());
        prop_assert_eq!(all_rows(&base, &spec), before);
    }

    /// Immutability: any valid diversion — in force or not — leaves
    /// the base feed exactly as it was.
    #[test]
    fn base_feed_is_never_mutated(
        (spec, d_raw) in spec_and_div(),
    ) {
        let base = Gtfs::from_zip_bytes(&build_zip(&spec)).expect("parse");
        let before = all_rows(&base, &spec);

        let d = make_diversion(&spec, &d_raw);
        validate_diversion(&base, &d).expect("generated diversion is valid");
        let _ = apply_diversions(&base, query_date(), &[d]).expect("apply");

        prop_assert_eq!(all_rows(&base, &spec), before);
    }

    /// Commutativity on disjoint scopes: diversions on different
    /// routes cannot affect the same trip, so permuting the list
    /// yields identical patched rows. (When scopes overlap, order is
    /// meaningful by design — see `apply_diversions` docs — so the
    /// disjointness restriction IS the law, not a test limitation.)
    #[test]
    fn disjoint_diversions_commute(
        (spec, a, b) in spec_and_disjoint_pair(),
    ) {
        // Both in force and provably disjoint by construction:
        // different routes cannot affect the same trip.
        let base = Gtfs::from_zip_bytes(&build_zip(&spec)).expect("parse");
        let da = make_diversion(&spec, &a);
        let db = make_diversion(&spec, &b);
        validate_diversion(&base, &da).expect("valid a");
        validate_diversion(&base, &db).expect("valid b");

        let ab = apply_diversions(&base, query_date(), &[da.clone(), db.clone()]).expect("apply ab");
        let ba = apply_diversions(&base, query_date(), &[db, da]).expect("apply ba");
        prop_assert_eq!(all_rows(&ab, &spec), all_rows(&ba, &spec));
    }

    /// Oracle: for one diversion in force, EVERY trip is predictable
    /// from the spec. Matching trips (route, direction when named,
    /// anchors in order) lose their interior skipped stop, renumber
    /// from 0, and shift from the last anchor onward; every other
    /// trip keeps its rows and its original sequences. The base feed
    /// is unchanged.
    #[test]
    fn one_diversion_matches_the_oracle_on_every_trip(
        (spec, d_raw) in spec_and_div_in_force(),
    ) {
        let base = Gtfs::from_zip_bytes(&build_zip(&spec)).expect("parse");
        let d = make_diversion(&spec, &d_raw);
        validate_diversion(&base, &d).expect("generated diversion is valid");

        let patched = apply_diversions(&base, query_date(), &[d]).expect("apply");

        for trip in 0..spec.trips.len() {
            let trip_id = format!("trip{trip}");
            let expected = oracle_rows(&spec, &d_raw, trip);
            prop_assert_eq!(rows(&patched, &trip_id), expected, "trip {}", trip_id);
        }
        prop_assert_eq!(all_rows(&base, &spec), (0..spec.trips.len()).map(|i| spec.trips[i].base_rows()).collect::<Vec<_>>());
    }
}
