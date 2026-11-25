//! Without a work rule, an imported layover is the whole observed gap, and
//! every delay would read infeasible (D-9, D-110). Every rule is optional,
//! and an agency's values are data, because nothing may couple to one labor
//! agreement (D-50, D-51). [`WorkRules::checked`] normalizes the document at
//! the kernel's load, so two orderings are one identity.
//!
//! [`suggest_blocks`] has no deadheads. A vehicle need is a block assignment,
//! not a count of concurrent trips, so each suggestion carries the edits that
//! adopt it and the peak it needs (D-46, D-107, D-109).

#![forbid(unsafe_code)]

mod recovery;
mod rules;
mod suggest;
pub mod load;

pub use recovery::{recovery, PatternRecovery, Recovery, RecoveryWait};
pub use rules::{required, MealBreak, Pay, RecoveryRule, RoutePair, RunRules, WorkRules, WorkRulesKind, SCHEMA_VERSION};
pub use suggest::{suggest_blocks, SuggestedBlock, SuggestedTrip, Suggestion, Unblocked, UnblockedReason, Wait};
