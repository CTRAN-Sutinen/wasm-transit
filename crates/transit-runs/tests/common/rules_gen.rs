//! Generated work rules that sample every rule type, none or many of each.

use std::collections::BTreeMap;

use proptest::prelude::*;
use proptest::sample::{select, subsequence};
use transit_runs::*;

pub const MEASURES: [&str; 3] = ["m0", "m1", "m2"];
pub const ACTIVITIES: [Activity; 8] = [
    Activity::Report,
    Activity::Clear,
    Activity::Revenue,
    Activity::Deadhead,
    Activity::Layover,
    Activity::Travel,
    Activity::Standby,
    Activity::Gap,
];
pub const EVENTS: [Event; 6] = [
    Event::PullOut,
    Event::PullIn,
    Event::ReliefOn,
    Event::ReliefOff,
    Event::PieceStart,
    Event::PieceEnd,
];
pub const DAYS: [Day; 7] = [
    Day::Mon,
    Day::Tue,
    Day::Wed,
    Day::Thu,
    Day::Fri,
    Day::Sat,
    Day::Sun,
];
pub const KINDS: [RuleKind; 7] = [
    RuleKind::Allowances,
    RuleKind::Gaps,
    RuleKind::Limits,
    RuleKind::Breaks,
    RuleKind::Premiums,
    RuleKind::Guarantees,
    RuleKind::Travel,
];

pub fn measure() -> impl Strategy<Value = String> {
    select(&MEASURES[..]).prop_map(String::from)
}

pub fn allowance() -> impl Strategy<Value = Allowance> {
    (select(&EVENTS[..]), 1u32..3600, select(&ACTIVITIES[..])).prop_map(
        |(event, secs, activity)| Allowance {
            event,
            secs,
            activity,
            cites: None,
            extra: Default::default(),
        },
    )
}

pub fn gaps() -> impl Strategy<Value = Vec<GapBand>> {
    prop_oneof![
        Just(vec![]),
        (
            prop::collection::btree_set(1u32..20_000, 0..3),
            prop::collection::vec(0u32..=100, 4)
        )
            .prop_map(|(froms, pcts)| {
                std::iter::once(0)
                    .chain(froms)
                    .zip(pcts)
                    .map(|(from_secs, paid_pct)| GapBand {
                        from_secs,
                        paid_pct,
                        cites: None,
                        extra: Default::default(),
                    })
                    .collect()
            }),
    ]
}

pub fn limit() -> impl Strategy<Value = Limit> {
    let quantity = select(
        &[
            Quantity::Measure,
            Quantity::Spread,
            Quantity::Piece,
            Quantity::Pieces,
            Quantity::Gap,
            Quantity::DaysWorked,
            Quantity::Rest,
        ][..],
    );
    (
        quantity,
        measure(),
        0u32..50_000,
        0u32..50_000,
        0usize..3,
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(quantity, m, a, b, bounds, cross, waivable)| {
            let (lo, hi) = (a.min(b), a.max(b));
            let (min, max) = match bounds {
                0 => (Some(lo), None),
                1 => (None, Some(hi)),
                _ => (Some(lo), Some(hi)),
            };
            let over = match quantity {
                Quantity::DaysWorked | Quantity::Rest => {
                    if cross {
                        Window::Week
                    } else {
                        Window::Rolling24h
                    }
                }
                Quantity::Measure | Quantity::Spread if cross => Window::Week,
                _ => Window::Run,
            };
            Limit {
                quantity,
                measure: (quantity == Quantity::Measure).then_some(m),
                min,
                max,
                over,
                strength: if waivable {
                    Strength::Waivable
                } else {
                    Strength::Hard
                },
                cites: None,
                extra: Default::default(),
            }
        })
}

pub fn break_rule() -> impl Strategy<Value = BreakRule> {
    (
        1u32..3600,
        0u32..20_000,
        1u32..20_000,
        proptest::option::of(measure()),
        1u32..4,
        any::<bool>(),
        proptest::option::of((measure(), 0u32..40_000)),
    )
        .prop_map(
            |(min_secs, from, len, start, count, paid, when)| BreakRule {
                min_secs,
                window_from_secs: from,
                window_to_secs: from + len,
                from: if start.is_some() {
                    BreakFrom::MeasureStart
                } else {
                    BreakFrom::Report
                },
                measure: start,
                count,
                paid,
                when_measure: when.as_ref().map(|w| w.0.clone()),
                when_over_secs: when.map(|w| w.1),
                cites: None,
                extra: Default::default(),
            },
        )
}

pub fn premium(classes: Vec<String>) -> impl Strategy<Value = Premium> {
    let ons: Vec<TriggerOn> = [
        TriggerOn::Measure,
        TriggerOn::Spread,
        TriggerOn::TimeOfDay,
        TriggerOn::DayOfWeek,
    ]
    .into_iter()
    .chain((!classes.is_empty()).then_some(TriggerOn::Class))
    .collect();
    let class = if classes.is_empty() {
        Just(String::new()).boxed()
    } else {
        select(classes).boxed()
    };
    (
        select(ons),
        measure(),
        measure(),
        0u32..=1000,
        0u32..80_000,
        1u32..20_000,
        subsequence(&DAYS[..], 1..=7),
        class,
    )
        .prop_map(|(on, pays, m, pct, at, len, days, class)| {
            let mut p = Premium {
                on,
                pays,
                pct,
                measure: None,
                after_secs: None,
                from_secs: None,
                to_secs: None,
                days: vec![],
                class: None,
                cites: None,
                extra: Default::default(),
            };
            match on {
                TriggerOn::Measure => (p.measure, p.after_secs) = (Some(m), Some(at)),
                TriggerOn::Spread => p.after_secs = Some(at),
                TriggerOn::TimeOfDay => (p.from_secs, p.to_secs) = (Some(at), Some(at + len)),
                TriggerOn::DayOfWeek => p.days = days,
                TriggerOn::Class => p.class = Some(class),
            }
            p
        })
}

pub fn guarantee() -> impl Strategy<Value = Guarantee> {
    (
        select(&[Per::Run, Per::Report, Per::Day, Per::Week][..]),
        1u32..40_000,
        measure(),
    )
        .prop_map(|(per, min_secs, counts)| Guarantee {
            per,
            min_secs,
            counts,
            cites: None,
            extra: Default::default(),
        })
}

pub fn travel() -> impl Strategy<Value = TravelRule> {
    (
        select(&[TravelAt::DayEnd, TravelAt::BetweenPieces][..]),
        0u32..10_000,
        0u32..2_000,
        0usize..3,
        1u32..3600,
        1u32..100,
        proptest::option::of(0u32..=1000),
    )
        .prop_map(|(at, beyond_m, within_m, how, fixed, kph, pct)| {
            let (timed, fixed_secs, speed_kph) = match how {
                0 => (Timing::NextTrip, None, None),
                1 => (Timing::Fixed, Some(fixed), None),
                _ => (Timing::Speed, None, Some(kph)),
            };
            TravelRule {
                at,
                beyond_m,
                within_m,
                timed,
                fixed_secs,
                speed_kph,
                pct,
                cites: None,
                extra: Default::default(),
            }
        })
}

pub fn dedup<T: Ord>(mut v: Vec<T>) -> Vec<T> {
    v.sort();
    v.dedup();
    v
}

pub fn rule_set(classes: Vec<String>) -> impl Strategy<Value = RuleSet> {
    (
        prop::collection::vec(allowance(), 0..3),
        gaps(),
        prop::collection::vec(limit(), 0..4),
        prop::collection::vec(break_rule(), 0..3),
        prop::collection::vec(premium(classes), 0..3),
        prop::collection::vec(guarantee(), 0..3),
        prop::collection::vec(travel(), 0..2),
    )
        .prop_map(
            |(allowances, gaps, limits, breaks, premiums, guarantees, travel)| RuleSet {
                allowances: dedup(allowances),
                gaps,
                limits: dedup(limits),
                breaks: dedup(breaks),
                premiums: dedup(premiums),
                guarantees: dedup(guarantees),
                travel: dedup(travel),
            },
        )
}

pub fn class(id: String, ids: Vec<String>) -> impl Strategy<Value = RunClass> {
    (
        prop::collection::vec(limit(), 0..3),
        subsequence(&KINDS[..], 0..=7),
        rule_set(ids),
    )
        .prop_map(move |(conditions, replaces, rules)| {
            let replaces = replaces
                .into_iter()
                .filter(|&k| k != RuleKind::Gaps || !rules.gaps.is_empty())
                .collect();
            RunClass {
                id: id.clone(),
                conditions: dedup(conditions),
                replaces,
                rules,
                cites: None,
                extra: Default::default(),
            }
        })
}

pub fn mix(ids: Vec<String>) -> impl Strategy<Value = Vec<MixRule>> {
    if ids.is_empty() {
        return Just(vec![]).boxed();
    }
    prop::collection::vec(
        (
            subsequence(ids, 1..=1),
            prop::collection::vec(0u32..=100, 3),
            1usize..8,
            any::<bool>(),
        ),
        0..3,
    )
    .prop_map(|rules| {
        let rules = rules.into_iter().map(|(classes, mut pcts, which, paid)| {
            pcts.sort();
            let pick = |i: usize, v: u32| (which >> i & 1 == 1).then_some(v);
            MixRule {
                classes,
                of: if paid {
                    ShareOf::PaidTime
                } else {
                    ShareOf::Runs
                },
                min_pct: pick(0, pcts[0]),
                target_pct: pick(1, pcts[1]),
                max_pct: pick(2, pcts[2]),
                cites: None,
                extra: Default::default(),
            }
        });
        dedup(rules.collect())
    })
    .boxed()
}

pub fn work_rules() -> impl Strategy<Value = WorkRules> {
    (0usize..4).prop_flat_map(|k| {
        let ids: Vec<String> = (0..k).map(|i| format!("c{i}")).collect();
        let measures =
            prop::collection::vec(subsequence(&ACTIVITIES[..], 1..=8), 3).prop_map(|lists| {
                MEASURES
                    .iter()
                    .map(|m| m.to_string())
                    .zip(lists)
                    .collect::<BTreeMap<_, _>>()
            });
        let classes = ids
            .iter()
            .map(|id| class(id.clone(), ids.clone()))
            .collect::<Vec<_>>();
        let maximize = if ids.is_empty() {
            Just(vec![]).boxed()
        } else {
            subsequence(ids.clone(), 0..=ids.len()).boxed()
        };
        (
            measures,
            rule_set(ids.clone()),
            any::<bool>(),
            classes,
            mix(ids),
            proptest::option::of(maximize),
            any::<bool>(),
        )
            .prop_map(
                |(measures, rules, sum, classes, mix, maximize, minimize_premium)| WorkRules {
                    measures,
                    rules,
                    combine: if sum { Combine::Sum } else { Combine::Highest },
                    classes,
                    mix,
                    objective: maximize.map(|maximize| Objective {
                        maximize,
                        minimize_premium,
                        extra: Default::default(),
                    }),
                    ..Default::default()
                },
            )
    })
}
