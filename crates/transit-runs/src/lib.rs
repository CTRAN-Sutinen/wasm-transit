//! transit-runs — the runs layer, beside the network (D-16).
//!
//! Blocks answer what the vehicles do; the runs layer answers the
//! rules the work is cut under and, from them, how the work is cut.
//! What is here:
//!
//! * [`WorkRules`] — THE work-rule document (D-50): generic, every
//!   rule optional, an agency's values as data — the recovery a wait
//!   requires (D-9: minutes and/or a share of the run, per route or
//!   service type), the route pairs a vehicle may interline (D-46),
//!   the run rules (platform, spread, report and clear, pieces, meal
//!   breaks, relief points), and pay. A kernel document kind
//!   (`work_rules`), addressed by its canonical hash after
//!   [`WorkRules::checked`].
//! * [`recovery`] — every wait between a block's adjacent trips on a
//!   day: the recovery required there and the slack above it, folded
//!   per pattern. The requirement is
//!   `transit_network::required_recovery` — the rules win, else the
//!   terminal's floor — the same rule validation applies.
//! * [`suggest_blocks`] — the trips no block carries, chained into
//!   suggested blocks the planner adopts (the answer's edits) or
//!   edits, with each wait's slack and the peak the adoption needs
//!   (D-46).
//!
//! The runs cut itself (runs from blocks under the run rules) is not
//! here yet (`docs/units/runs-layer.md`). Pure: no clock, no I/O.

#![forbid(unsafe_code)]

mod recovery;
mod rules;
mod suggest;

pub use recovery::{recovery, PatternRecovery, Recovery, RecoveryWait};
pub use rules::{required, MealBreak, Pay, RecoveryRule, RoutePair, RunRules, WorkRules, SCHEMA_VERSION};
pub use suggest::{suggest_blocks, SuggestedBlock, SuggestedTrip, Suggestion, Unblocked, UnblockedReason, Wait};
