//! Service combinations (D-94): the service types that run
//! together on the same dates. The oracle walks every date and calls `runs_on`
//! for each type.
//!
//! 1. Every date on which a type with trips runs belongs to exactly one
//!    combination, whose members are those types.
//! 2. A type with no trips is never a member (D-94).
//! 3. Members are sorted, and the id joins them with " + ".
//! 4. Spans break where the next date is more than seven days after the last.
//! 5. `first_service_date` is the earliest date that a type with trips runs.

use std::collections::{BTreeMap, BTreeSet};

use transit_date::{Days, Date};
use proptest::prelude::*;
use transit_network::{
    first_service_date, service_combination, service_combinations, CalendarException, CalendarRule, DateRange, DayState, Network, ServiceType,
    WeekdaySet,
};

const EPOCH: Date = Date::from_ymd_opt(2026, 1, 1).unwrap();

fn base() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap()
}

#[derive(Clone, Debug)]
struct GenType {
    days: [bool; 7],
    start: u16,
    len: u16,
    exceptions: Vec<(u16, bool)>,
    trips: bool,
}

fn gen_type() -> impl Strategy<Value = GenType> {
    (
        proptest::array::uniform7(any::<bool>()),
        0u16..60,
        0u16..90,
        proptest::collection::vec((0u16..160, any::<bool>()), 0..4),
        proptest::bool::weighted(0.8),
    )
        .prop_map(|(days, start, len, exceptions, trips)| GenType { days, start, len, exceptions, trips })
}

/// Each generated type with trips runs a copy of the base's schedule.
fn network(types: &[GenType]) -> Network {
    let mut net = base();
    let template = net.pattern_trips.values().next().unwrap().clone();
    net.service_types.clear();
    net.pattern_trips.clear();
    net.blocks.clear();
    for (i, t) in types.iter().enumerate() {
        let id = format!("T{i}");
        let mut seen = BTreeSet::new();
        let rule = CalendarRule {
            days: WeekdaySet {
                mon: t.days[0],
                tue: t.days[1],
                wed: t.days[2],
                thu: t.days[3],
                fri: t.days[4],
                sat: t.days[5],
                sun: t.days[6],
            },
            ranges: vec![DateRange {
                start: EPOCH + Days::new(t.start as i64),
                end: EPOCH + Days::new((t.start + t.len) as i64),
                extra: Default::default(),
            }],
            exceptions: t
                .exceptions
                .iter()
                .filter(|(d, _)| seen.insert(*d))
                .map(|&(d, added)| CalendarException {
                    date: EPOCH + Days::new(d as i64),
                    state: if added { DayState::Added } else { DayState::Removed },
                    extra: Default::default(),
                })
                .collect(),
                extra: Default::default(),
        };
        net.service_types.insert(id.clone(), ServiceType { name: format!("Type {i}"), rule, extra: Default::default() });
        if t.trips {
            let mut pt = template.clone();
            pt.service_type = id.clone();
            for trip in &mut pt.trips {
                trip.id = format!("{id}:{}", trip.id);
            }
            net.pattern_trips.insert(format!("{id}-sched"), pt);
        }
    }
    net
}

proptest! {
    #[test]
    fn the_first_service_date_is_the_earliest_date_anything_runs(types in proptest::collection::vec(gen_type(), 0..=5)) {
        let net = network(&types);
        let with_trips: BTreeSet<String> = net.pattern_trips.values()
            .filter(|pt| !pt.trips.is_empty())
            .map(|pt| pt.service_type.clone())
            .collect();
        let oracle = (0..200i64)
            .map(|day| EPOCH + Days::new(day))
            .find(|date| net.service_types.iter().any(|(id, st)| with_trips.contains(id) && st.rule.runs_on(*date)));
        prop_assert_eq!(first_service_date(&net), oracle);
    }

    #[test]
    fn combinations_partition_the_dates(types in proptest::collection::vec(gen_type(), 0..=5)) {
        let net = network(&types);
        let combos = service_combinations(&net);

        let with_trips: BTreeSet<String> = net.pattern_trips.values()
            .filter(|pt| !pt.trips.is_empty())
            .map(|pt| pt.service_type.clone())
            .collect();
        let mut running: BTreeMap<Date, BTreeSet<String>> = BTreeMap::new();
        for day in 0..200i64 {
            let date = EPOCH + Days::new(day);
            let set: BTreeSet<String> = net.service_types.iter()
                .filter(|(id, st)| with_trips.contains(*id) && st.rule.runs_on(date))
                .map(|(id, _)| id.clone())
                .collect();
            if !set.is_empty() {
                running.insert(date, set);
            }
        }

        let mut placed: BTreeMap<String, usize> = BTreeMap::new();
        for (i, c) in combos.iter().enumerate() {
            prop_assert!(!c.members.is_empty(), "a combination has members");
            prop_assert!(!c.dates.is_empty(), "a combination holds on some date");
            let members: BTreeSet<String> = c.members.iter().cloned().collect();
            for d in &c.dates {
                let date = Date::parse(d).unwrap();
                prop_assert_eq!(running.get(&date), Some(&members), "{} runs exactly {:?}", d, c.members);
                prop_assert!(placed.insert(d.clone(), i).is_none(), "{} is in one combination", d);
            }
            prop_assert!(c.members.iter().all(|m| with_trips.contains(m)), "{:?} carry trips", c.members);
            let mut sorted = c.members.clone();
            sorted.sort();
            sorted.dedup();
            prop_assert_eq!(&sorted, &c.members, "members sorted and distinct");
            prop_assert_eq!(&c.id, &c.members.join(" + "));
            prop_assert!(c.dates.windows(2).all(|w| w[0] < w[1]), "dates sorted and distinct");
            let dates: Vec<Date> = c.dates.iter().map(|d| Date::parse(d).unwrap()).collect();
            let mut spans: Vec<(Date, Date, u32)> = Vec::new();
            for &d in &dates {
                match spans.last_mut() {
                    Some(s) if (d - s.1).num_days() <= 7 => { s.1 = d; s.2 += 1; }
                    _ => spans.push((d, d, 1)),
                }
            }
            prop_assert_eq!(c.spans.len(), spans.len(), "{} spans", c.id);
            for (got, want) in c.spans.iter().zip(&spans) {
                prop_assert_eq!(&got.start, &want.0.to_string());
                prop_assert_eq!(&got.end, &want.1.to_string());
                prop_assert_eq!(got.days, want.2);
            }
            let mut reversed = c.members.clone();
            reversed.reverse();
            prop_assert_eq!(service_combination(&net, &reversed).map(|f| f.id), Some(c.id.clone()));
        }
        prop_assert_eq!(placed.len(), running.len(), "every running date is placed");

        let ids: BTreeSet<&String> = combos.iter().map(|c| &c.id).collect();
        prop_assert_eq!(ids.len(), combos.len());

        for a in net.service_types.keys() {
            for b in net.service_types.keys() {
                if a >= b { continue; }
                let together = running.values().any(|s| s.len() == 2 && s.contains(a) && s.contains(b));
                prop_assert_eq!(
                    service_combination(&net, &[a.clone(), b.clone()]).is_some(), together,
                    "{} + {} holds only where exactly they run", a, b
                );
            }
        }
        prop_assert!(service_combination(&net, &[]).is_none());
        prop_assert!(service_combination(&net, &["ghost".to_string()]).is_none());
    }

    /// A date is an address, not an input: every date of a combination
    /// materializes the same day.
    #[test]
    fn every_date_of_a_combination_materializes_the_same_day(types in proptest::collection::vec(gen_type(), 0..=5)) {
        let net = network(&types);
        for c in service_combinations(&net) {
            let day = |d: &String| transit_network::materialize(&net, Date::parse(d).unwrap()).unwrap();
            let first = day(&c.dates[0]);
            for d in &c.dates[1..] {
                prop_assert_eq!(&day(d), &first, "{} materializes {}'s day", d, c.dates[0]);
            }
        }
    }
}

/// A base weekday with /30 in one month and /31 the next is two combinations,
/// base + /30 and base + /31. A holiday type that runs where the base is
/// removed is a combination of its own.
#[test]
fn successive_overlays_are_separate_combinations() {
    let mut net = base();
    let template = net.pattern_trips.values().next().unwrap().clone();
    net.service_types.clear();
    net.pattern_trips.clear();
    net.blocks.clear();
    let weekdays = WeekdaySet { mon: true, tue: true, wed: true, thu: true, fri: true, sat: false, sun: false };
    let d = |s: &str| Date::parse(s).unwrap();
    let ty = |start: &str, end: &str, exceptions: Vec<CalendarException>| ServiceType {
        name: String::new(),
        rule: CalendarRule { days: weekdays.clone(), ranges: vec![DateRange { start: d(start), end: d(end), extra: Default::default() }], exceptions, extra: Default::default() },
        extra: Default::default(),
    };
    let holiday = CalendarException { date: d("2026-10-01"), state: DayState::Removed, extra: Default::default() };
    net.service_types.insert("WKDY".into(), ty("2026-09-14", "2026-10-30", vec![holiday.clone()]));
    net.service_types.insert("WKDY/30".into(), ty("2026-09-14", "2026-10-09", vec![holiday]));
    net.service_types.insert("WKDY/31".into(), ty("2026-10-12", "2026-10-30", vec![]));
    net.service_types.insert(
        "HOL".into(),
        ServiceType {
            name: String::new(),
            rule: CalendarRule {
                days: WeekdaySet::default(),
                ranges: vec![],
                exceptions: vec![CalendarException { date: d("2026-10-01"), state: DayState::Added, extra: Default::default() }],
                extra: Default::default(),
            },
            extra: Default::default(),
        },
    );
    for id in ["WKDY", "WKDY/30", "WKDY/31", "HOL"] {
        let mut pt = template.clone();
        pt.service_type = id.into();
        net.pattern_trips.insert(format!("{id}-sched"), pt);
    }
    let combos = service_combinations(&net);
    let ids: Vec<&str> = combos.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["WKDY + WKDY/30", "WKDY + WKDY/31", "HOL"], "most days first");
    assert_eq!(combos[0].dates.len(), 19, "Sep 14 – Oct 9 weekdays, less the holiday");
    assert_eq!(combos[0].spans.len(), 1);
    assert_eq!((combos[0].spans[0].start.as_str(), combos[0].spans[0].end.as_str()), ("2026-09-14", "2026-10-09"));
    assert_eq!(combos[2].dates, ["2026-10-01"]);
    assert!(service_combination(&net, &["WKDY/30".into(), "WKDY/31".into()]).is_none());
    assert!(service_combination(&net, &["WKDY".into(), "WKDY/30".into(), "WKDY/31".into()]).is_none());
}
