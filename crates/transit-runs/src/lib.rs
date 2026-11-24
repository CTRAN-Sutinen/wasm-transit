//! transit-runs: the runs layer, beside the network (D-16). Pure: no clock,
//! no I/O. cap-runcut and cap-author are the seams. The runs cut itself is
//! open work (docs/units/runs-layer.md).
//!
//! * [`WorkRules`] is the one work-rule document (D-50), kernel kind
//!   `work_rules`. Every rule is optional and an agency's values are data,
//!   because nothing may couple to one labor agreement: recovery, the
//!   route pairs that may interline (D-46; a route follows itself, an
//!   unlisted pair never interlines), the run rules, and pay in time, no
//!   dollars (D-51). [`WorkRules::checked`] normalizes it at the kernel's
//!   load, so two orderings are one identity.
//! * Required recovery is `transit_network::required_recovery`: the work
//!   rule wins (minutes and/or a share of the run, the larger; the most
//!   specific rule answers), else the terminal's floor. Without a rule,
//!   an imported layover is the whole observed gap, and every delay would
//!   read infeasible (D-9, D-110). [`recovery`] shows each wait's
//!   requirement and slack; it is short exactly where validation says
//!   `layover_infeasible`.
//! * [`suggest_blocks`] chains the unblocked trips: each follows the
//!   vehicle that leaves it the least slack at the same stop or station,
//!   under the interline rules and the recovery. No deadheads yet. Its
//!   blocks are suggestions with the edits that adopt them and the peak
//!   they need, because a vehicle need is a block assignment, not a count
//!   of concurrent trips (D-46, D-107, D-109).

#![forbid(unsafe_code)]

mod recovery;
mod rules;
mod suggest;
pub mod load;

pub use recovery::{recovery, PatternRecovery, Recovery, RecoveryWait};
pub use rules::{required, MealBreak, Pay, RecoveryRule, RoutePair, RunRules, WorkRules, WorkRulesKind, SCHEMA_VERSION};
pub use suggest::{suggest_blocks, SuggestedBlock, SuggestedTrip, Suggestion, Unblocked, UnblockedReason, Wait};
