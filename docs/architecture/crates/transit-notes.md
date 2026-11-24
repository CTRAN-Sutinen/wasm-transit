# transit-notes

The notes document (D-214): the agency's internal notes about its day of service, in place of email. Anyone reads the notes. They never go to riders. cap-notes is the seam.

**A note** is a record by id. It has:
- its target: a trip instance (trip, service date, start), an edit record, a service alert, or a stop. A trip that the live feed cancels is a trip instance. The target never changes.
- its text.
- `written`: who wrote it and when. `edited`: who changed the text last and when.

`write` places one note: a new id is a new note, and a held id is an edit of its text. `checked` is what a write must pass: a blank id, text, or writer, a target that names no id, a date that is not "YYYY-MM-DD", or an edit earlier than the note. `on` and `between` are a target's notes and a span's notes.

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### A note carries its writer and time, and the server stamps them (transit-notes, cap-notes `write`, `POST /notes`)

- **Holds:** a note is a record of its own, in a document of its own. It is not a payload of the edit document, and it changes no service. No GTFS-rt feed and no rider page reads it.
- **Holds:** a note carries who wrote it and when (`written`), and who edited it last and when (`edited`). A reader needs no access to the log.
- **Holds:** a write is one note. The server runs `write` over the head with the writer it verified and its own clock. A page never names who wrote a note.
- **Holds:** nothing is deleted, and a note's target never changes. An edit replaces the text, and the log keeps the earlier state.
- **Because:** the notes replace dispatch's email, and customer service reads them without signing in. The log names each entry's writer, but reading it needs the audit rule. One note to a write also means that two people who note at once never conflict over the whole document.
- **Rejected:** the whole document posted under `from`, as the alerts are; a writer named by the page; the writer and time read from the log.
- **Decided:** D-214 (the shape: D-217).
