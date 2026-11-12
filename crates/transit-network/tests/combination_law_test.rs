//! The service COMBINATION laws (D-55): a planner's service group is
//! the set of service types that run together on the same dates, shown
//! with the dates on which it holds. Members that never share a day
//! are never pooled, and no date stands in for a combination — every
//! date it holds on runs exactly its members.
//!
//! The oracle walks every date independently (`runs_on` per type, per
//! date) and checks the answer against it:
//!
//! * **Partition** — every date on which a type with trips runs
//!   belongs to exactly one combination, whose members are exactly the
//!   types with trips that run that date; a date nothing runs on
//!   belongs to none.
//! * **Never pooled apart** — two members of a combination share every
//!   one of its dates (so types that never share a day never meet).
//! * **Trip-less types** — a type that carries no trips runs no
//!   service and is never a member (D-94).
//! * **Identity** — members sorted, the id their " + " join, dates
//!   sorted and distinct; `service_combination` finds a combination by
//!   its member set in any order and answers None for a set that holds
//!   on no date.
//! * **Spans** (D-94) — the dates, folded into spans: a new span starts
//!   where the next date it holds is more than seven days after the
//!   last; the spans' days sum to the dates.
//! * **First service date** (D-140) — `first_service_date` is the
//!   earliest date some type with trips runs, None when nothing runs:
//!   the date an imported feed picks on.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDate};
use proptest::prelude::*;
use transit_network::{
    first_service_date, service_combination, service_combinations, CalendarException, CalendarRule, DateRange, DayState, Network, ServiceType,
    WeekdaySet,
};

const EPOCH: NaiveDate = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

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

/// The base network with its one type replaced by the generated ones:
/// each type with trips runs a copy of the base's schedule.
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
                start: EPOCH + Duration::days(t.start as i64),
                end: EPOCH + Duration::days((t.start + t.len) as i64),
            }],
            exceptions: t
                .exceptions
                .iter()
                .filter(|(d, _)| seen.insert(*d))
                .map(|&(d, added)| CalendarException {
                    date: EPOCH + Duration::days(d as i64),
                    state: if added { DayState::Added } else { DayState::Removed },
                })
                .collect(),
        };
        net.service_types.insert(id.clone(), ServiceType { name: format!("Type {i}"), rule });
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
            .map(|day| EPOCH + Duration::days(day))
            .find(|date| net.service_types.iter().any(|(id, st)| with_trips.contains(id) && st.rule.runs_on(*date)));
        prop_assert_eq!(first_service_date(&net), oracle);
    }

    #[test]
    fn combinations_partition_the_dates(types in proptest::collection::vec(gen_type(), 0..=5)) {
        let net = network(&types);
        let combos = service_combinations(&net);

        // Oracle: per date, the types WITH TRIPS that run on it.
        let with_trips: BTreeSet<String> = net.pattern_trips.values()
            .filter(|pt| !pt.trips.is_empty())
            .map(|pt| pt.service_type.clone())
            .collect();
        let mut running: BTreeMap<NaiveDate, BTreeSet<String>> = BTreeMap::new();
        for day in 0..200i64 {
            let date = EPOCH + Duration::days(day);
            let set: BTreeSet<String> = net.service_types.iter()
                .filter(|(id, st)| with_trips.contains(*id) && st.rule.runs_on(date))
                .map(|(id, _)| id.clone())
                .collect();
            if !set.is_empty() {
                running.insert(date, set);
            }
        }

        // Partition: every running date in exactly one combination,
        // with exactly that date's members.
        let mut placed: BTreeMap<String, usize> = BTreeMap::new();
        for (i, c) in combos.iter().enumerate() {
            prop_assert!(!c.members.is_empty(), "a combination has members");
            prop_assert!(!c.dates.is_empty(), "a combination holds on some date");
            let members: BTreeSet<String> = c.members.iter().cloned().collect();
            for d in &c.dates {
                let date = NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap();
                prop_assert_eq!(running.get(&date), Some(&members), "{} runs exactly {:?}", d, c.members);
                prop_assert!(placed.insert(d.clone(), i).is_none(), "{} is in one combination", d);
            }
            // Trip-less types are never members.
            prop_assert!(c.members.iter().all(|m| with_trips.contains(m)), "{:?} carry trips", c.members);
            // Identity.
            let mut sorted = c.members.clone();
            sorted.sort();
            sorted.dedup();
            prop_assert_eq!(&sorted, &c.members, "members sorted and distinct");
            prop_assert_eq!(&c.id, &c.members.join(" + "));
            prop_assert!(c.dates.windows(2).all(|w| w[0] < w[1]), "dates sorted and distinct");
            // Spans: cover the dates, broken where the next is > 7 days on.
            let dates: Vec<NaiveDate> = c.dates.iter().map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap()).collect();
            let mut spans: Vec<(NaiveDate, NaiveDate, u32)> = Vec::new();
            for &d in &dates {
                match spans.last_mut() {
                    Some(s) if (d - s.1).num_days() <= 7 => { s.1 = d; s.2 += 1; }
                    _ => spans.push((d, d, 1)),
                }
            }
            prop_assert_eq!(c.spans.len(), spans.len(), "{} spans", c.id);
            for (got, want) in c.spans.iter().zip(&spans) {
                prop_assert_eq!(&got.start, &want.0.format("%Y-%m-%d").to_string());
                prop_assert_eq!(&got.end, &want.1.format("%Y-%m-%d").to_string());
                prop_assert_eq!(got.days, want.2);
            }
            // Lookup by member set, in any order.
            let mut reversed = c.members.clone();
            reversed.reverse();
            prop_assert_eq!(service_combination(&net, &reversed).map(|f| f.id), Some(c.id.clone()));
        }
        prop_assert_eq!(placed.len(), running.len(), "every running date is placed");

        // Distinct member sets: one row per combination.
        let ids: BTreeSet<&String> = combos.iter().map(|c| &c.id).collect();
        prop_assert_eq!(ids.len(), combos.len());

        // A set that never runs together answers None.
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
}

/// Two overlays in successive periods never pool (QAF-91's shape): a
/// base weekday with /30 in one month and /31 the next is two
/// combinations, base + /30 and base + /31, and no row holds /30 and
/// /31 together. A dated exception — the base removed on a holiday a
/// holiday type runs — is a combination of its own.
#[test]
fn successive_overlays_are_separate_combinations() {
    let mut net = base();
    let template = net.pattern_trips.values().next().unwrap().clone();
    net.service_types.clear();
    net.pattern_trips.clear();
    net.blocks.clear();
    let weekdays = WeekdaySet { mon: true, tue: true, wed: true, thu: true, fri: true, sat: false, sun: false };
    let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    let ty = |start: &str, end: &str, exceptions: Vec<CalendarException>| ServiceType {
        name: String::new(),
        rule: CalendarRule { days: weekdays.clone(), ranges: vec![DateRange { start: d(start), end: d(end) }], exceptions },
    };
    let holiday = CalendarException { date: d("2026-10-01"), state: DayState::Removed };
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
                exceptions: vec![CalendarException { date: d("2026-10-01"), state: DayState::Added }],
            },
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
