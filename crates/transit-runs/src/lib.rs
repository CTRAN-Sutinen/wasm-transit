//! Two documents hold the rules. The work rules hold what a labor agreement
//! binds, and the scheduling constraints hold the agency's own choices:
//! recovery, interline, and relief points. Recovery and interline judge
//! blocks before any run exists, so the recovery view and the suggested
//! blocking read only the scheduling constraints. Every rule is optional,
//! and an agency's values are data, because nothing may couple to one labor
//! agreement.
//!
//! The work rules hold any operator agreement, and any reading of one, as
//! data. Agreements change at each negotiation, and a memorandum or a
//! grievance changes how a clause is read. Each is a new document, and the
//! code changes only when an agreement needs a kind of rule that
//! [`WorkRules`] cannot express. The set of rule types is fixed, and a new
//! type is a new schema version. A document names its own measures as sets
//! of activities, because agreements mean different things by "drive",
//! "work", and "platform". The pay that the rules give is a planning
//! measure for comparing cuts, in time and not dollars. It is not payroll,
//! so an unknown field is kept and ignored, as in every kind. C-TRAN's
//! 2022–2025 agreement is a fixture (`crates/fixtures`, `work_rules.rs`),
//! never a default.
//!
//! [`judge`] judges and pays one run under the rules. [`cut_pieces`] cuts
//! a day's blocks into pieces, [`compose_runs`] composes pieces into runs,
//! and [`judge_runs`] judges a set of runs, whoever composed them: the
//! agency, an operator at the bid, or a planner by hand. [`cut_runs`] is
//! the two steps together. The cut is a heuristic. It builds runs that
//! break no rule where it can, because a waivable limit is the operator's
//! to waive.
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
mod cut;
mod recovery;
mod rules;
mod run;
mod suggest;
pub mod load;
pub mod version;

pub use recovery::{recovery, PatternRecovery, Recovery, RecoveryWait};
pub use cut::{
    compose_refs, compose_runs, cut_pieces, cut_runs, judge_runs, Composition, Cut, CutRun, DayBlocks, MixReport, NetworkPlaces, PieceRef, Pieces, RunRef,
    RunSet,
};
pub use run::{judge, meets, timeline, Finding, Judged, Piece, PieceTrip, Places, Span};
pub use constraints::{required, RecoveryRule, RoutePair, SchedulingConstraints, SchedulingConstraintsKind, CONSTRAINTS_VERSION};
pub use rules::{
    Activity, Allowance, BreakFrom, BreakRule, Combine, Day, Event, GapBand, Guarantee, Limit, MixRule, Objective, Per, Premium, Quantity,
    RuleKind, RuleSet, RunClass, ShareOf, Source, Strength, Timing, TravelAt, TravelRule, TriggerOn, Window, WorkRules, WorkRulesKind,
    SCHEMA_VERSION,
};
pub use suggest::{suggest_blocks, SuggestedBlock, SuggestedTrip, Suggestion, Unblocked, UnblockedReason, Wait};
