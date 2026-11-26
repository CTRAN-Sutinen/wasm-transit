//! Two documents hold the rules. The work rules hold what a labor agreement
//! binds, and the scheduling constraints hold the agency's own choices:
//! recovery, interline, and relief points. Recovery and interline judge
//! blocks before any run exists, so the recovery view and the suggested
//! blocking read only the scheduling constraints. Every rule is optional,
//! and an agency's values are data, because nothing may couple to one labor
//! agreement.
//!
//! Without a recovery rule, an imported layover is the whole observed gap,
//! and every delay would read infeasible (D-110). Each document's `checked`
//! normalizes it at the kernel's load, so two orderings are one identity.
//!
//! [`suggest_blocks`] has no deadheads. A vehicle need is a block assignment,
//! not a count of concurrent trips, so each suggestion carries the edits that
//! adopt it and the peak it needs (D-107, D-109).

#![forbid(unsafe_code)]

mod constraints;
mod recovery;
mod rules;
mod suggest;
pub mod load;
pub mod version;

pub use recovery::{recovery, PatternRecovery, Recovery, RecoveryWait};
pub use constraints::{required, RecoveryRule, RoutePair, SchedulingConstraints, SchedulingConstraintsKind, CONSTRAINTS_VERSION};
pub use rules::{MealBreak, Pay, RunRules, WorkRules, WorkRulesKind, SCHEMA_VERSION};
pub use suggest::{suggest_blocks, SuggestedBlock, SuggestedTrip, Suggestion, Unblocked, UnblockedReason, Wait};
