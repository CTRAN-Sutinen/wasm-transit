# cap-notes

The notes capability: what has the agency noted about this? transit-notes is the library.
- `validate` gives what a write must pass, or the canonical document and its hash. This is the document that `load_notes` answers.
- `write` writes one note, new or edited, stamped with the writer and the time that the host gives. With no document, the first note begins one.
- `on` gives one target's notes, oldest first.
- `between` gives the notes written in a time span, newest first.
- `receive` writes the notes log whole (D-228): each `write` over the document at the head, stamped with the writer and the clock of the entry's header, then the entry.
- `log_records` and `log_document` turn the document into the notes log's records and back.
