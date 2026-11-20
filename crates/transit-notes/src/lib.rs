//! The notes document (D-214): the agency's internal notes about its
//! day of service.
//!
//! A note names its target: a trip instance (trip, service date,
//! start), a record of the edit document, a service alert, or a stop. A
//! trip that the live feed cancels is a trip instance, so a note can
//! name it. Anyone can read the notes. They never go to riders: no
//! GTFS-rt feed and no rider page reads them.
//!
//! A note carries who wrote it and when, and who last edited it and
//! when. The writer is the verified sign-in subject that the host gives
//! the write. The time is the host's clock, in Unix seconds. So the
//! notes read with no access to the log.
//!
//! Nothing is deleted. An edit replaces the note's text, and the log
//! keeps every earlier state. A note's target never changes.
//!
//! [`Notes::write`] writes one note, new or edited. [`Notes::checked`] is
//! what a write must pass. It refuses only what makes a note wrong
//! (D-152). The notes do not read the network: a note on an id the
//! network does not hold is a note, as written.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The version this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

transit_version::kind_tag!(
    /// The notes document's kind tag: always `"notes"`.
    NotesKind = "notes"
);

impl transit_version::Versioned for Notes {
    const KIND: &'static str = "notes";
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

/// Every note, by its id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Notes {
    /// The kind tag and version (D-115); a copy without them is this
    /// kind, version 1.
    #[serde(default)]
    #[ts(type = "\"notes\"")]
    pub kind: NotesKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<1>,
    pub notes: BTreeMap<String, Note>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// One note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Note {
    /// What the note is about. It never changes.
    pub target: NoteTarget,
    /// The note's text. It is not blank.
    pub text: String,
    /// Who wrote the note first, and when.
    pub written: Stamp,
    /// Who changed the text last, and when. Absent until an edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub edited: Option<Stamp>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// A writer and a time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Stamp {
    /// The verified sign-in subject, or the tool, that wrote.
    pub by: String,
    /// The host's clock, Unix seconds.
    #[ts(type = "number")]
    pub at: u64,
}

/// What a note is about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoteTarget {
    /// A trip instance: the model's trip id, its service date
    /// ("YYYY-MM-DD"), and its start in seconds after the service-day
    /// anchor. A service date names its pick, so the instance is
    /// unambiguous.
    #[schemars(extend("x-references" = "trip_instance"))]
    Trip { trip: String, date: String, start_secs: u32 },
    /// A record of the edit document (a diversion or a trip cancel), by
    /// its record id.
    Edit {
        #[schemars(extend("x-references" = "edit_record"))]
        id: String,
    },
    /// A service alert, by its id.
    Alert {
        #[schemars(extend("x-references" = "service_alert"))]
        id: String,
    },
    /// A stop, by its lasting id.
    Stop {
        #[schemars(extend("x-references" = "stop"))]
        stop: String,
    },
}

/// A note in a list: its id and the note.
#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct NoteRow {
    pub id: String,
    pub note: Note,
}

/// Why a write may not store a note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct NoteRefusal {
    /// The note's id.
    pub note: String,
    pub problem: NoteProblem,
}

/// What makes a note wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoteProblem {
    /// The note has a blank id.
    NoId,
    /// The text is blank.
    BlankText,
    /// The writer is blank.
    NoWriter,
    /// The target names a blank id.
    NoTarget,
    /// The trip instance's date is not "YYYY-MM-DD".
    BadDate,
    /// The edit is earlier than the note.
    EditedBeforeWritten,
    /// A write names another target for a note that is held.
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

/// Every problem of one note, in the order of [`NoteProblem`].
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
    /// Note `id` written by `by` at `at` (Unix seconds). A new id is a
    /// new note, stamped `written`. A held id is an edit: its text is
    /// replaced and it is stamped `edited`, and its target must be the
    /// one held. The same text written again changes nothing. Every
    /// other note as it was. A refused write names every problem.
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

    /// The document, if a write may store it, or every reason it may
    /// not.
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

    /// The notes about `target`, oldest first (by when each was written,
    /// then by id).
    pub fn on(&self, target: &NoteTarget) -> Vec<NoteRow> {
        let mut rows: Vec<NoteRow> =
            self.notes.iter().filter(|(_, n)| &n.target == target).map(|(id, n)| NoteRow { id: id.clone(), note: n.clone() }).collect();
        rows.sort_by(|a, b| (a.note.written.at, &a.id).cmp(&(b.note.written.at, &b.id)));
        rows
    }

    /// The notes written in `[from, to)` (Unix seconds), newest first
    /// (by when each was written, then by id). The host gives the span:
    /// a day is its start and end in the agency's zone.
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
