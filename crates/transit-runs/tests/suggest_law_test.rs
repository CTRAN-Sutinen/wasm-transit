//! Suggested blocking (D-46): the trips no block carries,
//! chained into suggested blocks the planner adopts or edits — tuned
//! by the work rules (recovery, interlining per route pair), with the
//! slack each wait leaves and the peak the adopted blocking needs.
//!
//! The laws: every unassigned trip the day runs is in exactly one
//! suggested block (or named as one the suggestion cannot place);
//! every wait is at one place (a stop, or a bay of the same station —
//! there are no deadheads yet), interlines only a pair the rules
//! allow, and is at least the recovery required; the answer's edits
//! adopt it — the next document carries the authored blocks
//! unchanged, validates clean under the same rules, and its fleet
//! peak is the answer's.
//!
//! API notes: `suggest_blocks(net, day, rules)` → `Suggestion {
//! blocks: [{block_id, trips: [{trip, route, start_secs, end_secs,
//! from_stop, to_stop, wait}]}], unblocked, authored_peak_vehicles,
//! peak_vehicles, peak_at_secs, added_vehicles, edits }`.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use proptest::prelude::*;
use transit_network::{apply_edits, materialize, validation_issues_under, Network};
use transit_runs::{suggest_blocks, RecoveryRule, RoutePair, Suggestion, WorkRules};

fn station(net: &Network, stop: &str) -> String {
    net.stops.get(stop).and_then(|s| s.parent.clone()).unwrap_or_else(|| stop.to_string())
}

fn rules(min_secs: Option<u32>, interline: bool) -> Option<WorkRules> {
    if min_secs.is_none() && !interline {
        return None;
    }
    Some(WorkRules {
        version: 1,
        recovery: min_secs.map(|m| RecoveryRule { route: None, service_type: None, min_secs: Some(m), pct_of_run: None }).into_iter().collect(),
        interline: if interline { vec![RoutePair { a: "5".into(), b: "6".into() }] } else { vec![] },
        ..Default::default()
    })
}

fn check(net: &Network, rules: Option<&WorkRules>) -> Result<Suggestion, TestCaseError> {
    let day = materialize(net, date()).unwrap();
    let s = suggest_blocks(net, &day, rules);
    let blocked: BTreeSet<&str> = net.blocks.values().flat_map(|b| b.trips.iter().map(|t| t.trip.as_str())).collect();
    let unassigned: BTreeSet<&str> = day.iter().map(|t| t.trip.as_str()).filter(|t| !blocked.contains(t)).collect();

    // Coverage: each unassigned trip once.
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for b in &s.blocks {
        for t in &b.trips {
            *seen.entry(t.trip.as_str()).or_default() += 1;
        }
    }
    for u in &s.unblocked {
        *seen.entry(u.trip.as_str()).or_default() += 1;
    }
    prop_assert_eq!(seen.keys().copied().collect::<BTreeSet<_>>(), unassigned);
    prop_assert!(seen.values().all(|&n| n == 1), "a trip suggested twice: {:?}", seen);

    // Every wait is legal and its numbers add up.
    for b in &s.blocks {
        prop_assert!(!net.blocks.contains_key(&b.block_id), "a suggestion never reuses a block id");
        for pair in b.trips.windows(2) {
            let (a, n) = (&pair[0], &pair[1]);
            let wait = a.wait.as_ref().expect("a trip with a next has a wait");
            prop_assert_eq!(station(net, &a.to_stop), station(net, &n.from_stop), "no deadheads");
            prop_assert!(a.route == n.route || rules.is_some_and(|r| r.interlines(&a.route, &n.route)), "interlined {} → {}", a.route, n.route);
            prop_assert_eq!(wait.gap_secs, n.start_secs - a.end_secs);
            prop_assert!(wait.gap_secs >= wait.required_secs);
            prop_assert_eq!(wait.slack_secs, wait.gap_secs - wait.required_secs);
            if let Some(r) = rules.and_then(|r| r.recovery.first()) {
                prop_assert!(wait.gap_secs >= r.min_secs.unwrap_or(0));
            }
        }
        prop_assert!(b.trips.last().unwrap().wait.is_none());
    }

    // Adoption: the edits make the next document.
    let adopted = apply_edits(net, &s.edits).map_err(|e| TestCaseError::fail(e.to_string()))?;
    for (id, block) in &net.blocks {
        prop_assert_eq!(adopted.blocks.get(id), Some(block), "authored blocks are untouched");
    }
    let issues = validation_issues_under(&adopted, &transit_runs::required(rules));
    prop_assert_eq!(issues, vec![]);
    let fleet = transit_dispatch::fleet_summary(&adopted, &day);
    prop_assert_eq!(s.peak_vehicles, fleet.peak_vehicles);
    prop_assert_eq!(s.peak_at_secs, fleet.peak_at_secs);
    let authored = transit_dispatch::fleet_summary(net, &day);
    prop_assert_eq!(s.authored_peak_vehicles, authored.peak_vehicles);
    prop_assert_eq!(s.added_vehicles, s.peak_vehicles.saturating_sub(authored.peak_vehicles));
    Ok(s)
}

fn trips() -> impl Strategy<Value = Vec<(&'static str, bool, u32)>> {
    proptest::collection::vec((prop_oneof![Just("5"), Just("6")], any::<bool>(), (5 * 60u32..22 * 60).prop_map(|m| m * 60)), 0..40)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn a_suggestion_covers_every_unassigned_trip_legally_and_adopts_clean(
        trips in trips(),
        min_secs in proptest::option::of(0u32..1800),
        interline in any::<bool>(),
    ) {
        let net = world(&trips, &[]);
        check(&net, rules(min_secs, interline).as_ref())?;
    }

    /// Authored blocks stay as authored: only the trips no block
    /// carries are chained, and the peak counts both.
    #[test]
    fn authored_blocks_are_kept(trips in trips(), min_secs in proptest::option::of(0u32..1800)) {
        let mut all = trips.clone();
        all.push(("5", true, 6 * H));
        all.push(("5", false, 7 * H));
        let mut net = world(&all, &[("auth", vec![trip_id("5", true, 6 * H), trip_id("5", false, 7 * H)])]);
        layover(&mut net, "auth", 0, 7 * H - 6 * H - RUN);
        prop_assume!(transit_network::validation_issues(&net).is_empty());
        check(&net, rules(min_secs, false).as_ref())?;
    }
}

#[test]
fn a_turn_shorter_than_the_rule_takes_another_vehicle() {
    // 5 out 08:00 → 08:20 at C; 5 in 08:30 from C. A 10-minute turn.
    let net = world(&[("5", true, 8 * H), ("5", false, 8 * H + 1800)], &[]);
    let one = check(&net, None).unwrap();
    assert_eq!(one.blocks.len(), 1);
    assert_eq!(one.peak_vehicles, 1);
    let wait = one.blocks[0].trips[0].wait.clone().unwrap();
    assert_eq!((wait.gap_secs, wait.required_secs, wait.slack_secs, wait.required_by), (600, 0, 600, None));
    let two = check(&net, rules(Some(900), false).as_ref()).unwrap();
    assert_eq!(two.blocks.len(), 2);
    assert_eq!(two.peak_vehicles, 1, "two blocks, never out at once");
}

#[test]
fn routes_interline_only_where_the_rules_allow_it() {
    // 6 out ends at C2, a bay of C's station; 5 in leaves C.
    let net = world(&[("6", true, 8 * H), ("5", false, 8 * H + 1800)], &[]);
    assert_eq!(check(&net, None).unwrap().blocks.len(), 2);
    let together = check(&net, rules(None, true).as_ref()).unwrap();
    assert_eq!(together.blocks.len(), 1, "a bay of the same station is the same place");
}

#[test]
fn a_proposal_names_the_vehicles_it_adds() {
    // The base runs one vehicle back and forth; the proposal adds an
    // overlapping trip, which no free vehicle can take.
    let base = [("5", true, 8 * H), ("5", false, 9 * H)];
    let mut trips = base.to_vec();
    trips.push(("5", true, 8 * H + 600));
    let mut net = world(&trips, &[("b", vec![trip_id("5", true, 8 * H), trip_id("5", false, 9 * H)])]);
    layover(&mut net, "b", 0, H - RUN);
    let s = check(&net, None).unwrap();
    assert_eq!((s.authored_peak_vehicles, s.peak_vehicles, s.added_vehicles), (1, 2, 1));
}
