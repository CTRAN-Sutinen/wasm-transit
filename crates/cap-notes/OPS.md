# notes

What the agency has noted about its day of service (library: transit-notes). The notes document holds the agency's internal notes. Dispatch, the communications staff, and customer service write them in place of email, and anyone reads them. Notes never go to riders: no GTFS-rt feed and no rider page reads them.

A note has these parts:

- its target: a trip instance (`trip`: the model's trip id, its service date, and its start), an edit record (`edit`: a diversion or a trip cancel, by record id), a service alert (`alert`, by id), or a stop (`stop`, by its lasting id). A trip that the live feed cancels is a trip instance, so a note can name it. The target never changes.
- its text, which is not blank.
- `written`: who wrote it first, and when.
- `edited`: who changed the text last, and when. It is absent until an edit.

The writer is the verified sign-in subject, or the tool, that the host gives. The time is the host's clock, in Unix seconds. So a reader needs no access to the log to see who wrote a note.

Nothing is deleted. An edit replaces the text, and the notes log keeps every earlier state. The notes do not read the network: a note on an id that the network does not hold is a note, as written.

A notes document registers on the kernel's `load_notes`. The load is refused unless `validate` would pass the document.

## validate

What a write must pass. The answer is every refusal, each naming its note and its problem, or the canonical document and its hash. The module is the hash authority: a host stores exactly those bytes.

A note is refused when its id, text, or writer is blank, when its target names a blank id, when a trip instance's date is not "YYYY-MM-DD", or when its edit is earlier than the note. Nothing else is refused.

## write

The document with note `id` written by `by` at `at` (Unix seconds, the host's clock). The host gives its verified writer. A page never names itself.

- A new id is a new note, stamped `written`.
- A held id is an edit. Its text is replaced, and it is stamped `edited`. Its `target` must be the one held, or the write is refused (`target_changed`). An edit earlier than the note is refused.
- The same text written again changes nothing.

Every other note stays as it was. A refused write answers its refusals and no document. A document absent (`notes_hash` omitted) is an empty one.

## on

The notes about one `target`, oldest first: by when each was written, then by id. Use it for one trip's, edit's, alert's, or stop's notes. A document absent is an empty one.

## between

The notes written in `[from, to)` (Unix seconds), newest first: by when each was written, then by id. The host gives the span. A day is its start and its end in the agency's zone. A document absent is an empty one.

## log_records

The document as the notes log's records: a `note` record for each note, and the document's own record, which keeps the fields outside its collection. A document left out gives no records. The log capability's `compose` takes the answer.

## log_document

The notes document assembled from the notes log's records, registered and answered as its canonical bytes and hash (`document_hash`). A record in another version is refused, naming both versions.
