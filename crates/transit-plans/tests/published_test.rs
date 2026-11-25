//! The published plan set (kind `plans`): one canonical document, one byte
//! sequence for one set of picks, and a chain it holds is valid (D-75, D-76).

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_plans::{
    in_force, in_use, publish, resolve, retire, PickStatus, PlanInfo, PlanIssue, PublishedPlans, PUBLISHED_SCHEMA_VERSION,
};

fn info(id: &str, from: Option<&str>) -> PlanInfo {
    PlanInfo { id: id.into(), name: format!("{id} pick"), effective_from: from.map(Into::into) }
}

fn hash(n: u8) -> String {
    format!("sha256:{}", format!("{n:02x}").repeat(32))
}

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

/// A "today" before every fixture date, so every pick is a future one.
fn early() -> NaiveDate {
    date("2000-01-01")
}

fn published(steps: &[(PlanInfo, String)]) -> PublishedPlans {
    steps.iter().fold(PublishedPlans::empty(), |set, (plan, h)| publish(&set, plan, h, early()).expect("valid publish"))
}

#[test]
fn the_empty_set_carries_its_version_and_resolves_nothing() {
    let set = PublishedPlans::empty();
    assert_eq!(set.schema_version, PUBLISHED_SCHEMA_VERSION, "D-62: the version travels inside the document");
    assert!(set.plans.is_empty());
    assert_eq!(in_force(&set, date("2026-11-05")).unwrap(), None);
}

#[test]
fn the_pick_in_force_names_its_network() {
    let set = published(&[
        (info("spring", Some("2026-03-01")), hash(1)),
        (info("fall", Some("2026-09-07")), hash(2)),
    ]);
    assert_eq!(in_force(&set, date("2026-01-01")).unwrap(), None, "no pick has taken effect yet");
    let spring = in_force(&set, date("2026-03-01")).unwrap().expect("the pick date itself is in force");
    assert_eq!((spring.id.as_str(), spring.network_hash.as_str()), ("spring", hash(1).as_str()));
    let fall = in_force(&set, date("2026-11-05")).unwrap().unwrap();
    assert_eq!((fall.id.as_str(), fall.network_hash.as_str()), ("fall", hash(2).as_str()));
}

#[test]
fn republishing_a_pick_replaces_it() {
    let set = published(&[
        (info("fall", Some("2026-09-07")), hash(1)),
        (info("fall", Some("2026-09-14")), hash(2)),
    ]);
    assert_eq!(set.plans.len(), 1, "one entry per pick id: a republish is the pick's next revision");
    assert_eq!(set.plans[0].network_hash, hash(2));
    assert_eq!(set.plans[0].effective_from, "2026-09-14", "a pick's date is a target that moves");
}

#[test]
fn publishing_never_mutates_its_input() {
    let before = published(&[(info("spring", Some("2026-03-01")), hash(1))]);
    let copy = before.clone();
    let _ = publish(&before, &info("fall", Some("2026-09-07")), &hash(2), early()).unwrap();
    assert_eq!(before, copy);
}

#[test]
fn a_pick_that_would_break_the_chain_is_refused() {
    let set = published(&[(info("spring", Some("2026-03-01")), hash(1))]);

    let issues = publish(&set, &info("", Some("2026-09-07")), &hash(2), early()).unwrap_err();
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::UnversionedPlan)), "{issues:?}");

    let issues = publish(&set, &info("fall", None), &hash(2), early()).unwrap_err();
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::PlanMissingEffectiveDate { .. })), "{issues:?}");

    let issues = publish(&set, &info("fall", Some("2026-03-01")), &hash(2), early()).unwrap_err();
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::DuplicateEffectiveDate { .. })), "{issues:?}");

    assert!(publish(&set, &info("fall", Some("2026-13-01")), &hash(2), early()).is_err());
}

fn arb_steps() -> impl Strategy<Value = Vec<(PlanInfo, String)>> {
    prop::collection::btree_map(0u8..12, (0u32..400, 1u8..200), 0..8).prop_map(|picks| {
        let mut seen_days = std::collections::BTreeSet::new();
        picks
            .into_iter()
            .filter(|(_, (day, _))| seen_days.insert(*day))
            .map(|(id, (day, h))| {
                let d = date("2026-01-01") + chrono::Days::new(day as u64);
                (info(&format!("p{id}"), Some(&d.format("%Y-%m-%d").to_string())), hash(h))
            })
            .collect()
    })
}

proptest! {
    /// The picks in use are the pick in force and every published pick still
    /// to come. Each republishes under its own date today.
    #[test]
    fn the_picks_in_use_are_the_pick_in_force_and_those_to_come(
        steps in arb_steps(), day in 0u32..500, retire_first in any::<bool>()
    ) {
        let mut set = published(&steps);
        let today = date("2025-12-01") + chrono::Days::new(day as u64);
        if retire_first {
            let later = set.plans.iter().find(|p| date(&p.effective_from) > today).map(|p| p.id.clone());
            if let Some(id) = later {
                set = retire(&set, &id, today).unwrap();
            }
        }
        let used: Vec<String> = in_use(&set, today).unwrap().iter().map(|p| p.id.clone()).collect();
        let in_force_today = in_force(&set, today).unwrap().map(|p| p.id.clone());
        let expected: Vec<String> = set.plans.iter()
            .filter(|p| p.status == PickStatus::Published)
            .filter(|p| Some(&p.id) == in_force_today.as_ref() || date(&p.effective_from) > today)
            .map(|p| p.id.clone())
            .collect();
        prop_assert_eq!(&used, &expected);
        for later in 0..600u64 {
            if let Some(p) = in_force(&set, today + chrono::Days::new(later)).unwrap() {
                prop_assert!(used.contains(&p.id), "{} governs a day from today on", p.id);
            }
        }
        for pick in in_use(&set, today).unwrap() {
            let again = publish(&set, &info(&pick.id, Some(&pick.effective_from)), &hash(250), today);
            prop_assert!(again.is_ok(), "{} republishes under its own date: {:?}", pick.id, again);
        }
    }

    /// One set of picks is one byte sequence, whatever order it was published
    /// in.
    #[test]
    fn publish_order_does_not_matter(steps in arb_steps(), seed in any::<u64>()) {
        let mut shuffled = steps.clone();
        let n = shuffled.len();
        for i in 0..n {
            let j = (seed.wrapping_mul(i as u64 + 1).wrapping_add(7) % n.max(1) as u64) as usize;
            shuffled.swap(i, j);
        }
        prop_assert_eq!(published(&steps), published(&shuffled));
    }

    #[test]
    fn publish_is_idempotent(steps in arb_steps()) {
        let once = published(&steps);
        let twice = steps.iter().fold(once.clone(), |set, (p, h)| publish(&set, p, h, early()).unwrap());
        prop_assert_eq!(once, twice);
    }

    /// `in_force` is `resolve` over the same picks.
    #[test]
    fn in_force_agrees_with_the_chain(steps in arb_steps(), day in 0u32..500) {
        let set = published(&steps);
        let d = date("2025-12-01") + chrono::Days::new(day as u64);
        let infos: Vec<PlanInfo> = set.plans.iter().map(|p| info(&p.id, Some(&p.effective_from))).collect();
        let chain = resolve(&infos, d).unwrap();
        let found = in_force(&set, d).unwrap();
        prop_assert_eq!(found.map(|p| p.id.clone()), chain.clone());
        if let (Some(p), Some(id)) = (found, chain) {
            let (_, h) = steps.iter().find(|(i, _)| i.id == id).unwrap();
            prop_assert_eq!(&p.network_hash, h);
        }
    }
}


fn today() -> NaiveDate {
    date("2026-11-05")
}

fn fall_and_spring() -> PublishedPlans {
    published(&[
        (info("fall", Some("2026-09-07")), hash(1)),
        (info("spring", Some("2027-03-01")), hash(2)),
    ])
}

#[test]
fn retiring_a_future_pick_takes_it_out_of_force_and_keeps_it() {
    let set = fall_and_spring();
    let retired = retire(&set, "spring", today()).unwrap();
    let spring = retired.plans.iter().find(|p| p.id == "spring").expect("the tombstone stays in the set");
    assert_eq!(spring.status, PickStatus::Retired);
    assert_eq!(spring.network_hash, hash(2), "the record keeps what was published");
    let after = in_force(&retired, date("2027-03-02")).unwrap().unwrap();
    assert_eq!(after.id, "fall", "the pick before it governs on");
    assert_eq!(retire(&retired, "spring", today()).unwrap(), retired, "retiring twice is retiring once");
    assert_eq!(set, fall_and_spring(), "the input is never mutated");
}

#[test]
fn a_pick_that_has_taken_effect_cannot_be_retired() {
    let set = fall_and_spring();
    let issues = retire(&set, "fall", today()).unwrap_err();
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::PickTookEffect { .. })), "{issues:?}");
    let issues = retire(&set, "spring", date("2027-03-01")).unwrap_err();
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::PickTookEffect { .. })), "{issues:?}");
    let issues = retire(&set, "nope", today()).unwrap_err();
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::UnknownPick { .. })), "{issues:?}");
}

#[test]
fn the_picks_in_use_leave_out_the_ones_that_stopped_governing() {
    let set = published(&[
        (info("spring", Some("2026-03-01")), hash(1)),
        (info("fall", Some("2026-09-07")), hash(2)),
        (info("winter", Some("2027-01-04")), hash(3)),
    ]);
    let ids = |d: &str| in_use(&set, date(d)).unwrap().iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids("2026-11-05"), ["fall", "winter"], "spring stopped governing when fall took effect");
    assert_eq!(ids("2026-01-01"), ["spring", "fall", "winter"], "before the first pick, every pick is to come");
    assert_eq!(ids("2027-02-01"), ["winter"]);
    assert!(in_use(&PublishedPlans::empty(), today()).unwrap().is_empty());
}

#[test]
fn republishing_a_retired_pick_restores_it() {
    let retired = retire(&fall_and_spring(), "spring", today()).unwrap();
    let restored = publish(&retired, &info("spring", Some("2027-03-01")), &hash(3), today()).unwrap();
    let spring = restored.plans.iter().find(|p| p.id == "spring").unwrap();
    assert_eq!((spring.status, spring.network_hash.as_str()), (PickStatus::Published, hash(3).as_str()));
    assert_eq!(in_force(&restored, date("2027-03-02")).unwrap().unwrap().id, "spring");
}

#[test]
fn a_retired_pick_is_not_a_chain_member() {
    let retired = retire(&fall_and_spring(), "spring", today()).unwrap();
    let other = publish(&retired, &info("spring-b", Some("2027-03-01")), &hash(4), today()).unwrap();
    assert_eq!(in_force(&other, date("2027-03-01")).unwrap().unwrap().id, "spring-b");
    let issues = publish(&other, &info("spring", Some("2027-03-01")), &hash(2), today()).unwrap_err();
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::DuplicateEffectiveDate { .. })), "{issues:?}");
}

#[test]
fn a_new_pick_never_lands_before_today() {
    let set = fall_and_spring();
    let issues = publish(&set, &info("winter", Some("2026-11-04")), &hash(5), today()).unwrap_err();
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::PickDatedInPast { .. })), "{issues:?}");
    let now = publish(&set, &info("winter", Some("2026-11-05")), &hash(5), today()).unwrap();
    assert_eq!(in_force(&now, today()).unwrap().unwrap().id, "winter");
}

#[test]
fn a_pick_never_moves_into_the_past() {
    let set = fall_and_spring();
    let issues = publish(&set, &info("spring", Some("2026-10-01")), &hash(2), today()).unwrap_err();
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::PickDatedInPast { .. })), "{issues:?}");
    let issues = publish(&set, &info("fall", Some("2026-09-01")), &hash(1), today()).unwrap_err();
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::PickDatedInPast { .. })), "{issues:?}");
    assert!(publish(&set, &info("spring", Some("2027-04-01")), &hash(2), today()).is_ok());
}

#[test]
fn a_correction_under_its_own_date_always_publishes() {
    let set = fall_and_spring();
    let fixed = publish(&set, &info("fall", Some("2026-09-07")), &hash(9), today()).unwrap();
    assert_eq!(in_force(&fixed, today()).unwrap().unwrap().network_hash, hash(9));
}

#[test]
fn restoring_a_pick_whose_date_has_passed_is_refused() {
    let retired = retire(&fall_and_spring(), "spring", today()).unwrap();
    let issues = publish(&retired, &info("spring", Some("2027-03-01")), &hash(2), date("2027-03-05")).unwrap_err();
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::PickDatedInPast { .. })), "{issues:?}");
}

proptest! {
    /// A retired pick never governs.
    #[test]
    fn retired_picks_never_resolve(steps in arb_steps(), retire_mask in any::<u16>(), day in 0u32..500) {
        let mut set = published(&steps);
        for (i, (plan, _)) in steps.iter().enumerate() {
            if retire_mask & (1 << (i % 16)) != 0 {
                set = retire(&set, &plan.id, early()).unwrap();
            }
        }
        let d = date("2025-12-01") + chrono::Days::new(day as u64);
        let live: Vec<PlanInfo> = set.plans.iter()
            .filter(|p| p.status == PickStatus::Published)
            .map(|p| info(&p.id, Some(&p.effective_from)))
            .collect();
        let found = in_force(&set, d).unwrap();
        prop_assert_eq!(found.map(|p| p.id.clone()), resolve(&live, d).unwrap());
        if let Some(p) = found {
            prop_assert_eq!(p.status, PickStatus::Published);
        }
    }
}
