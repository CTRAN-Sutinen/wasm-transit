//! The notes document (D-214): what a write must pass, a note written
//! and edited, nothing deleted, a target's notes, and the notes of a
//! time span.
//!
//! The laws come from the user's intent (2026-11-19): dispatch keeps
//! internal notes in place of email; a note names a trip instance, an
//! edit record, an alert, or a stop; anyone with the write role writes
//! one; a note is never deleted, and a correction is a new state of the
//! note (the log keeps the earlier text).

use proptest::prelude::*;
use transit_notes::*;

fn trip(t: &str, start: u32) -> NoteTarget {
    NoteTarget::Trip { trip: t.into(), date: "2026-11-19".into(), start_secs: start }
}

fn alert(id: &str) -> NoteTarget {
    NoteTarget::Alert { id: id.into() }
}

fn problems(r: Result<Notes, Vec<NoteRefusal>>) -> Vec<NoteProblem> {
    r.err().unwrap_or_default().into_iter().map(|r| r.problem).collect()
}

fn written(notes: &Notes, id: &str) -> Note {
    notes.notes.get(id).cloned().expect("the note is held")
}

// ---------------------------------------------------------------------
// A note written, and edited.
// ---------------------------------------------------------------------

#[test]
fn a_new_note_is_stamped_with_its_writer_and_time() {
    let notes = Notes::default().write("n1", trip("t10", 28800), "No operator.", "r.alvarez", 1_795_120_000).unwrap();
    let n = written(&notes, "n1");
    assert_eq!(n.target, trip("t10", 28800));
    assert_eq!(n.text, "No operator.");
    assert_eq!(n.written, Stamp { by: "r.alvarez".into(), at: 1_795_120_000 });
    assert_eq!(n.edited, None);
}

#[test]
fn an_edit_replaces_the_text_and_keeps_who_wrote_it_first() {
    let notes = Notes::default().write("n1", trip("t10", 28800), "No operator.", "r.alvarez", 100).unwrap();
    let notes = notes.write("n1", trip("t10", 28800), "No operator. Next trip covered.", "k.moore", 160).unwrap();
    let n = written(&notes, "n1");
    assert_eq!(n.text, "No operator. Next trip covered.");
    assert_eq!(n.written, Stamp { by: "r.alvarez".into(), at: 100 });
    assert_eq!(n.edited, Some(Stamp { by: "k.moore".into(), at: 160 }));
}

#[test]
fn the_same_text_written_again_changes_nothing() {
    let notes = Notes::default().write("n1", alert("a1"), "Posted.", "j.tran", 100).unwrap();
    let again = notes.write("n1", alert("a1"), "Posted.", "j.tran", 200).unwrap();
    assert_eq!(again, notes);
}

#[test]
fn a_note_keeps_its_target() {
    let notes = Notes::default().write("n1", trip("t10", 28800), "No operator.", "r.alvarez", 100).unwrap();
    assert_eq!(problems(notes.write("n1", trip("t20", 30000), "No operator.", "r.alvarez", 160)), vec![NoteProblem::TargetChanged]);
}

#[test]
fn an_edit_is_not_earlier_than_the_note() {
    let notes = Notes::default().write("n1", alert("a1"), "Posted.", "j.tran", 100).unwrap();
    assert_eq!(problems(notes.write("n1", alert("a1"), "Posted at 1:40.", "j.tran", 99)), vec![NoteProblem::EditedBeforeWritten]);
}

// ---------------------------------------------------------------------
// What a write must pass: only what makes a note wrong (D-152).
// ---------------------------------------------------------------------

#[test]
fn a_write_refuses_what_makes_a_note_wrong() {
    let empty = Notes::default();
    assert_eq!(problems(empty.write("n1", alert("a1"), "  ", "j.tran", 1)), vec![NoteProblem::BlankText]);
    assert_eq!(problems(empty.write("n1", alert("a1"), "Posted.", " ", 1)), vec![NoteProblem::NoWriter]);
    assert_eq!(problems(empty.write(" ", alert("a1"), "Posted.", "j.tran", 1)), vec![NoteProblem::NoId]);
    assert_eq!(problems(empty.write("n1", alert(""), "Posted.", "j.tran", 1)), vec![NoteProblem::NoTarget]);
    assert_eq!(problems(empty.write("n1", NoteTarget::Stop { stop: "".into() }, "Closed.", "j.tran", 1)), vec![NoteProblem::NoTarget]);
    let bad_date = NoteTarget::Trip { trip: "t10".into(), date: "11/19/2026".into(), start_secs: 0 };
    assert_eq!(problems(empty.write("n1", bad_date, "Cancelled.", "j.tran", 1)), vec![NoteProblem::BadDate]);
}

#[test]
fn a_target_the_network_does_not_have_is_not_refused() {
    // The notes do not read the network: a note on a trip or a stop the
    // network does not hold is a note, as written.
    let notes = Notes::default().write("n1", trip("no-such-trip", 0), "Odd one.", "r.alvarez", 1);
    assert!(notes.is_ok());
}

#[test]
fn checked_refuses_an_edit_earlier_than_the_note() {
    let mut notes = Notes::default().write("n1", alert("a1"), "Posted.", "j.tran", 100).unwrap();
    notes.notes.get_mut("n1").unwrap().edited = Some(Stamp { by: "j.tran".into(), at: 50 });
    assert_eq!(problems(notes.checked()), vec![NoteProblem::EditedBeforeWritten]);
}

// ---------------------------------------------------------------------
// A target's notes, and the notes of a time span.
// ---------------------------------------------------------------------

fn sample() -> Notes {
    Notes::default()
        .write("b", trip("t10", 28800), "Two calls from riders.", "k.moore", 200)
        .unwrap()
        .write("a", trip("t10", 28800), "No operator.", "r.alvarez", 300)
        .unwrap()
        .write("c", alert("a1"), "Alert posted.", "j.tran", 100)
        .unwrap()
        .write("d", trip("t10", 30000), "Late leaving.", "r.alvarez", 250)
        .unwrap()
}

#[test]
fn a_targets_notes_are_oldest_first() {
    let ids: Vec<String> = sample().on(&trip("t10", 28800)).into_iter().map(|r| r.id).collect();
    assert_eq!(ids, vec!["b", "a"], "the other instance of t10 is another target");
}

#[test]
fn a_spans_notes_are_newest_first() {
    let ids: Vec<String> = sample().between(150, 300).into_iter().map(|r| r.id).collect();
    assert_eq!(ids, vec!["d", "b"], "written in [from, to): 300 is outside, 100 is before");
}

// ---------------------------------------------------------------------
// Properties.
// ---------------------------------------------------------------------

fn any_target() -> impl Strategy<Value = NoteTarget> {
    prop_oneof![
        (0u8..3, 0u32..3).prop_map(|(t, s)| NoteTarget::Trip { trip: format!("t{t}"), date: "2026-11-19".into(), start_secs: s * 600 }),
        (0u8..3).prop_map(|e| NoteTarget::Edit { id: format!("e{e}") }),
        (0u8..3).prop_map(|a| NoteTarget::Alert { id: format!("a{a}") }),
        (0u8..3).prop_map(|s| NoteTarget::Stop { stop: format!("s{s}") }),
    ]
}

/// A run of writes: (id, target, text, at). Some are refused (a changed
/// target, an earlier edit); a refused write leaves the document as it
/// was.
fn any_writes() -> impl Strategy<Value = Vec<(u8, NoteTarget, u8, u64)>> {
    prop::collection::vec((0u8..6, any_target(), 0u8..3, 0u64..1000), 0..24)
}

fn run(writes: &[(u8, NoteTarget, u8, u64)]) -> Vec<Notes> {
    let mut docs = vec![Notes::default()];
    for (id, target, text, at) in writes {
        let held = docs.last().unwrap().clone();
        let next = held.write(&format!("n{id}"), target.clone(), &format!("text {text}"), "w", *at).unwrap_or(held);
        docs.push(next);
    }
    docs
}

proptest! {
    #[test]
    fn nothing_is_deleted(writes in any_writes()) {
        let docs = run(&writes);
        for pair in docs.windows(2) {
            for id in pair[0].notes.keys() {
                prop_assert!(pair[1].notes.contains_key(id), "note {id} left the document");
            }
        }
    }

    #[test]
    fn a_target_never_changes(writes in any_writes()) {
        let docs = run(&writes);
        for pair in docs.windows(2) {
            for (id, n) in &pair[0].notes {
                prop_assert_eq!(&pair[1].notes[id].target, &n.target);
                prop_assert_eq!(&pair[1].notes[id].written, &n.written);
            }
        }
    }

    #[test]
    fn every_written_document_passes_the_check(writes in any_writes()) {
        for d in run(&writes) {
            prop_assert!(d.clone().checked().is_ok());
        }
    }

    #[test]
    fn on_answers_exactly_the_targets_notes_in_time_order(writes in any_writes(), target in any_target()) {
        let d = run(&writes).pop().unwrap();
        let rows = d.on(&target);
        let want: std::collections::BTreeSet<&String> = d.notes.iter().filter(|(_, n)| n.target == target).map(|(id, _)| id).collect();
        let got: std::collections::BTreeSet<&String> = rows.iter().map(|r| &r.id).collect();
        prop_assert_eq!(got, want);
        for pair in rows.windows(2) {
            prop_assert!((pair[0].note.written.at, &pair[0].id) <= (pair[1].note.written.at, &pair[1].id));
        }
    }

    #[test]
    fn between_answers_exactly_the_spans_notes_newest_first(writes in any_writes(), from in 0u64..1000, len in 0u64..1000) {
        let d = run(&writes).pop().unwrap();
        let to = from + len;
        let rows = d.between(from, to);
        let want: std::collections::BTreeSet<&String> = d.notes.iter().filter(|(_, n)| from <= n.written.at && n.written.at < to).map(|(id, _)| id).collect();
        let got: std::collections::BTreeSet<&String> = rows.iter().map(|r| &r.id).collect();
        prop_assert_eq!(got, want);
        for pair in rows.windows(2) {
            prop_assert!((pair[0].note.written.at, &pair[1].id) >= (pair[1].note.written.at, &pair[0].id));
        }
    }
}
