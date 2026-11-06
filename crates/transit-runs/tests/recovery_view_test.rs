//! The recovery view: every wait between a block's adjacent
//! trips on a day, its run before it, the recovery required there
//! (D-9: the rules' requirement wins, else the terminal's floor), and
//! the slack above it — folded per pattern into layover-to-runtime.
//!
//! The laws: a wait is short exactly when validation reports the
//! layover infeasible under the same rules (one rule, two readers);
//! slack is the gap less the requirement; a pattern's totals are its
//! waits' sums.
//!
//! API notes: `recovery(net, day, rules)` → `Recovery { waits:
//! [{block_id, after_trip, trip, route, pattern, terminal,
//! start_secs, run_secs, gap_secs, required_secs, required_by,
//! slack_secs, recovery_pct}], patterns: [{pattern, route, waits,
//! run_secs, gap_secs, recovery_pct, short, min_slack_secs}] }`.

mod common;

use common::*;
use proptest::prelude::*;
use transit_network::{materialize, validation_issues_under, Departs, Network, NetworkError, RecoverySource};
use transit_runs::{recovery, RecoveryRule, WorkRules};

/// Two vehicles shuttling on 5: b1 08:00 out, 09:00 back (a 40-minute
/// wait at C, authored); b2 10:00 out, 10:30 back (a 10-minute wait,
/// authored). C's floor is 600.
fn shuttle() -> Network {
    let trips = [("5", true, 8 * H), ("5", false, 9 * H), ("5", true, 10 * H), ("5", false, 10 * H + 1800)];
    let mut net = world(
        &trips,
        &[
            ("b1", vec![trip_id("5", true, 8 * H), trip_id("5", false, 9 * H)]),
            ("b2", vec![trip_id("5", true, 10 * H), trip_id("5", false, 10 * H + 1800)]),
        ],
    );
    layover(&mut net, "b1", 0, 2400);
    layover(&mut net, "b2", 0, 600);
    net
}

fn shift(net: &mut Network, trip: &str, start: u32) {
    for pt in net.pattern_trips.values_mut() {
        for t in &mut pt.trips {
            if t.id == trip {
                t.departs = Departs::At { start_secs: start };
            }
        }
    }
}

#[test]
fn a_wait_says_what_is_required_and_what_is_slack() {
    let net = shuttle();
    let view = recovery(&net, &materialize(&net, date()).unwrap(), None);
    let rows: Vec<_> = view
        .waits
        .iter()
        .map(|w| (w.block_id.as_str(), w.terminal.as_str(), w.gap_secs, w.required_secs, w.required_by, w.slack_secs, w.recovery_pct))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("b1", "C", 2400, Some(600), Some(RecoverySource::Terminal), Some(1800), 200),
            ("b2", "C", 600, Some(600), Some(RecoverySource::Terminal), Some(0), 50),
        ]
    );
    assert_eq!(view.patterns.len(), 1);
    let p = &view.patterns[0];
    assert_eq!((p.pattern.as_str(), p.waits, p.run_secs, p.gap_secs, p.recovery_pct, p.short, p.min_slack_secs), ("5o", 2, 2400, 3000, 125, 0, Some(0)));
}

proptest! {
    /// One rule, two readers: a wait is short exactly where validation
    /// says the layover is infeasible, under the same rules.
    #[test]
    fn short_waits_are_the_validators_infeasible_layovers(
        back in 8 * H + RUN..=9 * H + 600,
        min_secs in proptest::option::of(0u32..3000),
    ) {
        let mut net = shuttle();
        shift(&mut net, &trip_id("5", false, 9 * H), back);
        let rules = min_secs.map(|m| WorkRules {
            version: 1,
            recovery: vec![RecoveryRule { route: None, service_type: None, min_secs: Some(m), pct_of_run: None }],
            ..Default::default()
        });
        let view = recovery(&net, &materialize(&net, date()).unwrap(), rules.as_ref());
        let mut short: Vec<(String, usize)> =
            view.waits.iter().filter(|w| w.slack_secs.is_some_and(|s| s < 0)).map(|w| (w.block_id.clone(), w.after_trip)).collect();
        short.sort();
        let mut infeasible: Vec<(String, usize)> = validation_issues_under(&net, &transit_runs::required(rules.as_ref()))
            .into_iter()
            .filter_map(|i| match i {
                NetworkError::LayoverInfeasible { block, after_trip, .. } => Some((block, after_trip)),
                _ => None,
            })
            .collect();
        infeasible.sort();
        prop_assert_eq!(short, infeasible);
        for w in &view.waits {
            prop_assert_eq!(w.slack_secs, w.required_secs.map(|r| w.gap_secs as i64 - r as i64));
        }
        let p = &view.patterns[0];
        prop_assert_eq!(p.waits as usize, view.waits.len());
        prop_assert_eq!(p.gap_secs, view.waits.iter().map(|w| w.gap_secs as u64).sum::<u64>());
        prop_assert_eq!(p.short as usize, view.waits.iter().filter(|w| w.slack_secs.is_some_and(|s| s < 0)).count());
    }
}
