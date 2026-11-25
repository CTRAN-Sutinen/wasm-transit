//! A write is one note ([`Notes::write`]), so two people who note at once
//! never conflict (D-214, D-217). [`Notes::checked`] refuses only what makes
//! a note wrong (D-152).

#![forbid(unsafe_code)]

pub mod load;

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const SCHEMA_VERSION: u32 = 1;

transit_version::kind_tag!(
    NotesKind = "notes"
);

impl transit_version::Versioned for Notes {
    const KIND: &'static str = "notes";
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Notes {
    #[serde(default)]
    #[ts(type = "\"notes\"")]
    pub kind: NotesKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<1>,
    pub notes: BTreeMap<String, Note>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Note {
    pub target: NoteTarget,
    pub text: String,
    pub written: Stamp,
    /// Absent until an edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub edited: Option<Stamp>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Stamp {
    pub by: String,
    /// Unix seconds.
    #[ts(type = "number")]
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoteTarget {
    /// `start_secs` is seconds after the service-day anchor. A service date
    /// names its pick, so the instance is unambiguous.
    #[schemars(extend("x-references" = "trip_instance"))]
    Trip { trip: String, date: String, start_secs: u32 },
    Edit {
        #[schemars(extend("x-references" = "edit_record"))]
        id: String,
    },
    Alert {
        #[schemars(extend("x-references" = "service_alert"))]
        id: String,
    },
    Stop {
        #[schemars(extend("x-references" = "stop"))]
        stop: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct NoteRow {
    pub id: String,
    pub note: Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct NoteRefusal {
    pub note: String,
    pub problem: NoteProblem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoteProblem {
    NoId,
    BlankText,
    NoWriter,
    NoTarget,
    BadDate,
    EditedBeforeWritten,
    TargetChanged,
}

impl fmt::Display for NoteRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let why = match &self.problem {
            NoteProblem::NoId => "the note has no id",
            NoteProblem::BlankText => "the text is blank",
            NoteProblem::NoWriter => "the writer is blank",
            NoteProblem::NoTarget => "the target names no id",
            NoteProblem::BadDate => "the trip's date is not YYYY-MM-DD",
            NoteProblem::EditedBeforeWritten => "the edit is earlier than the note",
            NoteProblem::TargetChanged => "a note's target cannot change",
        };
        write!(f, "note {:?}: {why}", self.note)
    }
}

fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn check(id: &str, n: &Note) -> Vec<NoteProblem> {
    let mut out = Vec::new();
    if blank(id) {
        out.push(NoteProblem::NoId);
    }
    if blank(&n.text) {
        out.push(NoteProblem::BlankText);
    }
    if blank(&n.written.by) || n.edited.as_ref().is_some_and(|e| blank(&e.by)) {
        out.push(NoteProblem::NoWriter);
    }
    let target_id = match &n.target {
        NoteTarget::Trip { trip, .. } => trip,
        NoteTarget::Edit { id } | NoteTarget::Alert { id } => id,
        NoteTarget::Stop { stop } => stop,
    };
    if blank(target_id) {
        out.push(NoteProblem::NoTarget);
    }
    if let NoteTarget::Trip { date, .. } = &n.target {
        if date.len() != 10 || chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
            out.push(NoteProblem::BadDate);
        }
    }
    if n.edited.as_ref().is_some_and(|e| e.at < n.written.at) {
        out.push(NoteProblem::EditedBeforeWritten);
    }
    out
}

impl Notes {
    pub fn write(&self, id: &str, target: NoteTarget, text: &str, by: &str, at: u64) -> Result<Notes, Vec<NoteRefusal>> {
        let refuse = |problems: Vec<NoteProblem>| problems.into_iter().map(|problem| NoteRefusal { note: id.to_string(), problem }).collect::<Vec<_>>();
        let stamp = Stamp { by: by.to_string(), at };
        let note = match self.notes.get(id) {
            None => Note { target, text: text.to_string(), written: stamp, edited: None, extra: Default::default() },
            Some(held) if held.target != target => return Err(refuse(vec![NoteProblem::TargetChanged])),
            Some(held) if held.text == text => return Ok(self.clone()),
            Some(held) => Note { text: text.to_string(), edited: Some(stamp), ..held.clone() },
        };
        let problems = check(id, &note);
        if !problems.is_empty() {
            return Err(refuse(problems));
        }
        let mut next = self.clone();
        next.notes.insert(id.to_string(), note);
        Ok(next)
    }

    pub fn checked(self) -> Result<Self, Vec<NoteRefusal>> {
        let refusals: Vec<NoteRefusal> = self
            .notes
            .iter()
            .flat_map(|(id, n)| check(id, n).into_iter().map(move |problem| NoteRefusal { note: id.clone(), problem }))
            .collect();
        if refusals.is_empty() {
            Ok(self)
        } else {
            Err(refusals)
        }
    }

    pub fn on(&self, target: &NoteTarget) -> Vec<NoteRow> {
        let mut rows: Vec<NoteRow> =
            self.notes.iter().filter(|(_, n)| &n.target == target).map(|(id, n)| NoteRow { id: id.clone(), note: n.clone() }).collect();
        rows.sort_by(|a, b| (a.note.written.at, &a.id).cmp(&(b.note.written.at, &b.id)));
        rows
    }

    pub fn between(&self, from: u64, to: u64) -> Vec<NoteRow> {
        let mut rows: Vec<NoteRow> = self
            .notes
            .iter()
            .filter(|(_, n)| from <= n.written.at && n.written.at < to)
            .map(|(id, n)| NoteRow { id: id.clone(), note: n.clone() })
            .collect();
        rows.sort_by(|a, b| b.note.written.at.cmp(&a.note.written.at).then_with(|| a.id.cmp(&b.id)));
        rows
    }
}
