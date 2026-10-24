//! Property-based tests (proptest).
//!
//! Instead of hand-picked fixtures, we generate random feeds and assert
//! laws that must hold for *every* feed. Three flavors are used here:
//!
//! * round-trip laws on pure functions (`parse_time`/`fmt_time`);
//! * oracle properties — the expected answer is recomputed *independently*
//!   from the generated spec (no shared code with the implementation), and
//!   the implementation must match exactly (`active_services`,
//!   `next_departures`, block chains);
//! * structural invariants — counts, sortedness, referential integrity.
//!
//! The payoff over example-based tests is proptest's shrinking: when a law
//! breaks, the generator reduces the feed to the smallest counterexample.

use std::collections::BTreeSet;
use std::io::Write;

use chrono::{Datelike, Duration, NaiveDate};
use proptest::collection::vec;
use proptest::prelude::*;
use transit_core::{fmt_time, parse_time, Calendar, CalendarDate, Gtfs, ServiceCalendar};

const EPOCH: NaiveDate = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

fn date_2026(day: u32) -> NaiveDate {
    EPOCH + Duration::days(day as i64)
}

fn day_index(date: NaiveDate) -> u32 {
    date.signed_duration_since(EPOCH).num_days() as u32
}

fn fmt_ymd(date: NaiveDate) -> String {
    format!("{:04}{:02}{:02}", date.year(), date.month(), date.day())
}

// ---------------------------------------------------------------------
// 1. Round-trip laws on pure functions
// ---------------------------------------------------------------------

proptest! {
    #[test]
    fn fmt_then_parse_roundtrips(secs in any::<u32>()) {
        // Every renderable second-count parses back to itself — including
        // after-midnight hours and absurd values.
        prop_assert_eq!(parse_time(&fmt_time(secs)), Some(secs));
    }

    #[test]
    fn parse_then_fmt_roundtrips(h in 0u32..100, m in 0u32..60, s in 0u32..60) {
        let text = format!("{h:02}:{m:02}:{s:02}");
        let secs = h * 3600 + m * 60 + s;
        prop_assert_eq!(parse_time(&text), Some(secs));
        prop_assert_eq!(fmt_time(secs), text);
    }
}

// ---------------------------------------------------------------------
// 2. Calendar laws, checked against independent computations
// ---------------------------------------------------------------------

fn every_day_calendar() -> Calendar {
    Calendar {
        service_id: "S".into(),
        monday: true,
        tuesday: true,
        wednesday: true,
        thursday: true,
        friday: true,
        saturday: true,
        sunday: true,
        start_date: EPOCH,
        end_date: date_2026(364),
    }
}

proptest! {
    #[test]
    fn calendar_pattern_matches_independent_computation(
        flags in vec(any::<bool>(), 7),
        start_day in 0u32..300,
        length in 1u32..60,
        day in 0u32..365,
    ) {
        let cal = Calendar {
            service_id: "S".into(),
            monday: flags[0],
            tuesday: flags[1],
            wednesday: flags[2],
            thursday: flags[3],
            friday: flags[4],
            saturday: flags[5],
            sunday: flags[6],
            start_date: date_2026(start_day),
            end_date: date_2026(start_day + length),
        };
        let sc = ServiceCalendar::new(vec![cal], vec![]);
        let date = date_2026(day);

        // Independent recomputation of the spec's rule.
        let weekday_flag = flags[date.weekday().num_days_from_monday() as usize];
        let expected =
            day >= start_day && day <= start_day + length && weekday_flag;
        prop_assert_eq!(sc.is_active("S", date), expected);
    }

    #[test]
    fn calendar_exceptions_override_everything(
        exception in 1u8..=2,
        day in 0u32..365,
    ) {
        let date = date_2026(day);
        let dates = vec![CalendarDate {
            service_id: "S".into(),
            date,
            exception_type: exception,
        }];
        let sc = ServiceCalendar::new(vec![every_day_calendar()], dates);

        // On the exception date the pattern is irrelevant.
        prop_assert_eq!(sc.is_active("S", date), exception == 1);
        // On any other date the every-day pattern applies.
        prop_assert!(sc.is_active("S", date_2026((day + 1) % 365)));
    }
}

// ---------------------------------------------------------------------
// 3. Random feeds
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TripSpec {
    route: usize,
    service: usize,
    start: u32, // first stop departure, seconds since service-day midnight
    duration: u32,
    first_stop: usize,
    block: Option<u16>,
}

#[derive(Debug, Clone)]
struct FeedSpec {
    n_routes: usize,
    n_stops: usize,
    /// (weekday flags mon..sun, start_day, end_day) — end inclusive.
    services: Vec<(Vec<bool>, u32, u32)>,
    /// (service id suffix, day, exception type) — ids may not exist in
    /// `services`: exception-only service keys are spec-legal.
    exceptions: Vec<(usize, u32, u8)>,
    trips: Vec<TripSpec>,
}

impl FeedSpec {
    fn service_id(&self, index: usize) -> String {
        format!("svc{index}")
    }

    fn stop_id(&self, index: usize) -> String {
        format!("s{index}")
    }

    fn trip_id(&self, index: usize) -> String {
        format!("trip{index}")
    }

    fn second_stop(&self, trip: &TripSpec) -> usize {
        (trip.first_stop + 1) % self.n_stops
    }
}

prop_compose! {
    fn trip_spec(n_routes: usize, n_services: usize, n_stops: usize)
        (route in 0usize..n_routes, service in 0usize..n_services,
         start in 0u32..90_000, duration in 0u32..3_600,
         first_stop in 0usize..n_stops, block in proptest::option::of(0u16..4))
    -> TripSpec {
        TripSpec { route, service, start, duration, first_stop, block }
    }
}

fn feed_spec() -> impl Strategy<Value = FeedSpec> {
    (
        1usize..=4, // routes
        1usize..=4, // stops
        // service patterns: weekday flags, range start, range length
        vec((vec(any::<bool>(), 7), 0u32..300, 1u32..60), 1..=3),
        // exceptions: service id suffix (may be out of range), day, type
        vec((any::<usize>(), 0u32..365, 1u8..=2), 0..=4),
    )
        .prop_flat_map(move |(n_routes, n_stops, services, exceptions)| {
            let n_services = services.len();
            vec(trip_spec(n_routes, n_services, n_stops), 1..=8).prop_map(
                move |trips| FeedSpec {
                    n_routes,
                    n_stops,
                    services: services.clone(),
                    exceptions: exceptions.clone(),
                    trips,
                },
            )
        })
}

/// Serialize a generated spec into a real GTFS zip.
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

    let mut s = String::from(
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n",
    );
    for (i, (flags, start_day, length)) in spec.services.iter().enumerate() {
        let as_flag = |b: bool| if b { "1" } else { "0" };
        s.push_str(&format!(
            "svc{},{},{},{},{},{},{},{},{},{}\n",
            i,
            as_flag(flags[0]),
            as_flag(flags[1]),
            as_flag(flags[2]),
            as_flag(flags[3]),
            as_flag(flags[4]),
            as_flag(flags[5]),
            as_flag(flags[6]),
            fmt_ymd(date_2026(*start_day)),
            fmt_ymd(date_2026(start_day + length)),
        ));
    }
    add(&mut w, "calendar.txt", &s);

    let mut s = String::from("service_id,date,exception_type\n");
    for (idx, day, exception) in &spec.exceptions {
        s.push_str(&format!(
            "svc{},{},{exception}\n",
            idx,
            fmt_ymd(date_2026(*day))
        ));
    }
    add(&mut w, "calendar_dates.txt", &s);

    let mut s = String::from("route_id,service_id,trip_id,trip_headsign,direction_id,block_id\n");
    for (i, t) in spec.trips.iter().enumerate() {
        let block = t.block.map(|b| b.to_string()).unwrap_or_default();
        s.push_str(&format!(
            "r{},svc{},trip{i},,0,{block}\n",
            t.route, t.service
        ));
    }
    add(&mut w, "trips.txt", &s);

    let mut s = String::from("trip_id,arrival_time,departure_time,stop_id,stop_sequence,stop_headsign\n");
    for (i, t) in spec.trips.iter().enumerate() {
        let end = t.start + t.duration;
        s.push_str(&format!(
            "trip{i},{},{},{},1,\n",
            fmt_time(t.start),
            fmt_time(t.start),
            spec.stop_id(t.first_stop)
        ));
        s.push_str(&format!(
            "trip{i},{},{},{},2,\n",
            fmt_time(end),
            fmt_time(end),
            spec.stop_id(spec.second_stop(t))
        ));
    }
    add(&mut w, "stop_times.txt", &s);

    w.finish().expect("finish zip").into_inner()
}

/// Oracle: which service keys run on `date`? Computed from the spec with
/// completely separate code (BTreeSet of id strings vs the
/// implementation's HashMaps of structs).
fn expected_active_services(spec: &FeedSpec, date: NaiveDate) -> BTreeSet<String> {
    let day = day_index(date);
    let mut active = BTreeSet::new();
    for (i, (flags, start_day, length)) in spec.services.iter().enumerate() {
        let weekday_flag = flags[date.weekday().num_days_from_monday() as usize];
        if day >= *start_day && day <= start_day + length && weekday_flag {
            active.insert(spec.service_id(i));
        }
    }
    for (idx, exception_day, exception) in &spec.exceptions {
        let id = spec.service_id(*idx);
        if exception_day == &day {
            if *exception == 1 {
                active.insert(id);
            } else {
                active.remove(&id);
            }
        }
    }
    active
}

// ---------------------------------------------------------------------
// 4. Oracle properties over whole generated feeds
// ---------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// The parser must recover exactly what the generator wrote.
    #[test]
    fn parsed_feed_matches_generator(spec in feed_spec()) {
        let gtfs = Gtfs::from_zip_bytes(&build_zip(&spec)).expect("generated feed parses");
        let summary = gtfs.summary();
        prop_assert_eq!(summary.routes, spec.n_routes);
        prop_assert_eq!(summary.stops, spec.n_stops);
        prop_assert_eq!(summary.trips, spec.trips.len());
        prop_assert_eq!(summary.stop_times, 2 * spec.trips.len());
        prop_assert_eq!(summary.agencies, 1);
    }

    /// Active service keys match the independent oracle — including days
    /// where several services run at once.
    #[test]
    fn active_services_match_oracle(spec in feed_spec(), day in 0u32..365) {
        let gtfs = Gtfs::from_zip_bytes(&build_zip(&spec)).expect("parse");
        let date = date_2026(day);
        let got: BTreeSet<String> = gtfs.active_services(date).into_iter().collect();
        prop_assert_eq!(got, expected_active_services(&spec, date));
    }

    /// Departure boards match the oracle exactly: same trips, same times,
    /// same order, limit and from-time honored.
    #[test]
    fn departures_match_oracle(
        spec in feed_spec(),
        stop_num in any::<u32>(),
        day in 0u32..365,
        from in any::<u32>(),
        limit in 1usize..8,
    ) {
        let gtfs = Gtfs::from_zip_bytes(&build_zip(&spec)).expect("parse");
        let stop = (stop_num as usize) % spec.n_stops;
        let date = date_2026(day);
        let active = expected_active_services(&spec, date);

        // Oracle: rows touching the stop, in feed order, filtered by
        // active service and from-time, stably sorted, truncated.
        let mut expected: Vec<(u32, usize)> = Vec::new();
        for (i, t) in spec.trips.iter().enumerate() {
            if !active.contains(&spec.service_id(t.service)) {
                continue;
            }
            if t.first_stop == stop {
                expected.push((t.start, i));
            }
            if spec.second_stop(t) == stop {
                expected.push((t.start + t.duration, i));
            }
        }
        expected.retain(|(time, _)| *time >= from);
        expected.sort_by_key(|(time, _)| *time); // stable, like the impl
        expected.truncate(limit);

        let deps = gtfs.next_departures(&spec.stop_id(stop), date, from, limit);
        prop_assert_eq!(deps.len(), expected.len());
        for (dep, (time, trip_idx)) in deps.iter().zip(&expected) {
            prop_assert_eq!(dep.departure_time, Some(*time));
            prop_assert_eq!(&dep.trip_id, &spec.trip_id(*trip_idx));
            prop_assert_eq!(&dep.service_id, &spec.service_id(spec.trips[*trip_idx].service));
            prop_assert_eq!(&dep.route_name, &format!("r{}", spec.trips[*trip_idx].route));
        }
    }

    /// Block chains match the oracle: order, endpoints, and layovers.
    #[test]
    fn block_chains_match_oracle(
        spec in feed_spec(),
        block_num in any::<u8>(),
        day in 0u32..365,
    ) {
        let gtfs = Gtfs::from_zip_bytes(&build_zip(&spec)).expect("parse");
        let block = (block_num % 4) as u16;
        let date = date_2026(day);
        let active = expected_active_services(&spec, date);

        let mut expected: Vec<(u32, u32, usize)> = spec
            .trips
            .iter()
            .enumerate()
            .filter(|(_, t)| t.block == Some(block))
            .filter(|(_, t)| active.contains(&spec.service_id(t.service)))
            .map(|(i, t)| (t.start, t.start + t.duration, i))
            .collect();
        expected.sort_by_key(|(start, _, _)| *start); // stable, like the impl

        let chain = gtfs.block_trips(&block.to_string(), date);
        prop_assert_eq!(chain.len(), expected.len());

        let mut previous_end: Option<u32> = None;
        for (trip, (start, end, trip_idx)) in chain.iter().zip(expected.iter()) {
            let (start, end) = (*start, *end);
            prop_assert_eq!(&trip.trip_id, &spec.trip_id(*trip_idx));
            prop_assert_eq!(&trip.service_id, &spec.service_id(spec.trips[*trip_idx].service));
            prop_assert_eq!(trip.start_time, Some(start));
            prop_assert_eq!(trip.end_time, Some(end));
            prop_assert_eq!(&trip.first_stop_name, &format!("Stop {}", spec.trips[*trip_idx].first_stop));
            prop_assert_eq!(&trip.last_stop_name, &format!("Stop {}", spec.second_stop(&spec.trips[*trip_idx])));

            let expected_layover = match previous_end {
                Some(prev) if start >= prev => Some(start - prev),
                _ => None,
            };
            prop_assert_eq!(trip.layover_secs, expected_layover);
            previous_end = Some(end);
        }
    }
}
