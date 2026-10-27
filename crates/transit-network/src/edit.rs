//! Authoring: the network document's edit vocabulary.
//!
//! A planner's change is a list of typed edits applied to a document
//! that is never mutated in place — `apply` answers the NEXT document
//! (the caller re-addresses it by hash and wholesale-replaces its
//! copy, the way every host owns documents). The list is atomic: one
//! edit that cannot apply rejects the whole change, naming it.
//!
//! The vocabulary is entity-level (upsert / remove per collection,
//! plus the plan frame): what an authoring UI, a script, and an agent
//! all express a change in. Removal refuses while anything still
//! references the entity — a dangling reference is a broken document,
//! and the model, not each host, is where that rule lives. Validity of
//! the RESULT is not enforced here: a document mid-edit may be
//! invalid, and validation reports it (`validation_issues`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{Block, Network, Pattern, PatternSchedule, PlanMeta, Route, Schedule, ServiceType, Stop};

/// One change to a network document. Upserts insert or replace the
/// whole entity under its id; removes refuse while referenced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "edit", rename_all = "snake_case")]
pub enum NetworkEdit {
    /// The plan frame: identity, name, pick date. Forking a plan into
    /// the next pick, or stamping a fresh import into the chain, is
    /// this edit.
    SetPlan { plan: PlanMeta },
    /// The document's display name.
    SetName { name: String },
    UpsertStop { id: String, stop: Stop },
    /// Refused while a pattern visits the stop, a layover names it,
    /// or another stop names it as parent.
    RemoveStop { id: String },
    UpsertRoute { id: String, route: Route },
    /// Refused while a pattern runs on the route.
    RemoveRoute { id: String },
    UpsertPattern { id: String, pattern: Pattern },
    /// Refused while a schedule runs the pattern.
    RemovePattern { id: String },
    UpsertServiceType { id: String, service_type: ServiceType },
    /// Refused while a schedule runs under the type.
    RemoveServiceType { id: String },
    UpsertSchedule { id: String, schedule: PatternSchedule },
    /// Move one trip of an exact-times schedule from `from_secs` to
    /// `to_secs`. The trip's identity is (schedule, start), so every
    /// block carrying it is carried along — a retime never strands
    /// the runcut (the starts stay sorted and unique).
    RetimeTrip { schedule: String, from_secs: u32, to_secs: u32 },
    /// Refused while a block carries one of its trips.
    RemoveSchedule { id: String },
    UpsertBlock { id: String, block: Block },
    RemoveBlock { id: String },
}

/// Why a change did not apply: the edit's position in the list and
/// what it collided with. The input document is untouched.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct EditError {
    pub index: usize,
    pub reason: String,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "edit {}: {}", self.index, self.reason)
    }
}

/// Apply `edits` in order to a copy of `network`; the input is never
/// touched. All or nothing: the first edit that cannot apply rejects
/// the change.
pub fn apply_edits(network: &Network, edits: &[NetworkEdit]) -> Result<Network, EditError> {
    let mut next = network.clone();
    for (index, edit) in edits.iter().enumerate() {
        apply_one(&mut next, edit).map_err(|reason| EditError { index, reason })?;
    }
    Ok(next)
}

fn apply_one(net: &mut Network, edit: &NetworkEdit) -> Result<(), String> {
    match edit {
        NetworkEdit::SetPlan { plan } => net.plan = plan.clone(),
        NetworkEdit::SetName { name } => net.name = name.clone(),
        NetworkEdit::UpsertStop { id, stop } => {
            net.stops.insert(id.clone(), stop.clone());
        }
        NetworkEdit::RemoveStop { id } => {
            exists(net.stops.contains_key(id), "stop", id)?;
            if let Some((p, _)) = net.patterns.iter().find(|(_, p)| p.stops.iter().any(|s| &s.stop == id)) {
                return Err(format!("stop {id} is visited by pattern {p}"));
            }
            if let Some((s, _)) = net.stops.iter().find(|(_, s)| s.parent.as_ref() == Some(id)) {
                return Err(format!("stop {id} is the parent of stop {s}"));
            }
            if let Some((b, _)) = net
                .blocks
                .iter()
                .find(|(_, b)| b.layovers.iter().any(|l| l.at.as_ref() == Some(id)))
            {
                return Err(format!("stop {id} is a layover point of block {b}"));
            }
            net.stops.remove(id);
        }
        NetworkEdit::UpsertRoute { id, route } => {
            net.routes.insert(id.clone(), route.clone());
        }
        NetworkEdit::RemoveRoute { id } => {
            exists(net.routes.contains_key(id), "route", id)?;
            if let Some((p, _)) = net.patterns.iter().find(|(_, p)| &p.route == id) {
                return Err(format!("route {id} carries pattern {p}"));
            }
            net.routes.remove(id);
        }
        NetworkEdit::UpsertPattern { id, pattern } => {
            net.patterns.insert(id.clone(), pattern.clone());
        }
        NetworkEdit::RemovePattern { id } => {
            exists(net.patterns.contains_key(id), "pattern", id)?;
            if let Some((s, _)) = net.schedules.iter().find(|(_, s)| &s.pattern == id) {
                return Err(format!("pattern {id} is run by schedule {s}"));
            }
            net.patterns.remove(id);
        }
        NetworkEdit::UpsertServiceType { id, service_type } => {
            net.service_types.insert(id.clone(), service_type.clone());
        }
        NetworkEdit::RemoveServiceType { id } => {
            exists(net.service_types.contains_key(id), "service type", id)?;
            if let Some((s, _)) = net.schedules.iter().find(|(_, s)| &s.service_type == id) {
                return Err(format!("service type {id} governs schedule {s}"));
            }
            net.service_types.remove(id);
        }
        NetworkEdit::UpsertSchedule { id, schedule } => {
            net.schedules.insert(id.clone(), schedule.clone());
        }
        NetworkEdit::RetimeTrip { schedule, from_secs, to_secs } => {
            let entry = net
                .schedules
                .get_mut(schedule)
                .ok_or_else(|| format!("unknown schedule {schedule}"))?;
            let Schedule::ExactTimes { starts } = &mut entry.schedule else {
                return Err(format!("schedule {schedule} runs headway bands — retime its bands instead"));
            };
            let at = starts
                .iter()
                .position(|s| s == from_secs)
                .ok_or_else(|| format!("schedule {schedule} runs no trip at {from_secs}"))?;
            if from_secs != to_secs && starts.contains(to_secs) {
                return Err(format!("schedule {schedule} already runs a trip at {to_secs}"));
            }
            starts[at] = *to_secs;
            starts.sort_unstable();
            for block in net.blocks.values_mut() {
                for trip in block.trips.iter_mut() {
                    if &trip.schedule == schedule && trip.start_secs == *from_secs {
                        trip.start_secs = *to_secs;
                    }
                }
            }
        }
        NetworkEdit::RemoveSchedule { id } => {
            exists(net.schedules.contains_key(id), "schedule", id)?;
            if let Some((b, _)) = net.blocks.iter().find(|(_, b)| b.trips.iter().any(|t| &t.schedule == id)) {
                return Err(format!("schedule {id} has trips in block {b}"));
            }
            net.schedules.remove(id);
        }
        NetworkEdit::UpsertBlock { id, block } => {
            net.blocks.insert(id.clone(), block.clone());
        }
        NetworkEdit::RemoveBlock { id } => {
            exists(net.blocks.contains_key(id), "block", id)?;
            net.blocks.remove(id);
        }
    }
    Ok(())
}

fn exists(present: bool, kind: &str, id: &str) -> Result<(), String> {
    if present {
        Ok(())
    } else {
        Err(format!("unknown {kind} {id}"))
    }
}
