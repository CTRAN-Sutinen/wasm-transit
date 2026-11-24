# transit-sync

The sync document: `EditRecord`, `FeedManifest`, `EditReport`, and the edit document. Every host shares it. cap-sync is the seam that the Go server calls. cap-divert is the authoring seam.

**The edit document** belongs to the dispatcher. It lives in the sync contract, not the GTFS codec. Its records carry exactly one payload: a `Diversion` or a `TripCancel`.
- A `TripCancel` takes a whole trip out of service on one date: one instance or all (D-45, D-101).
- An absent payload is omitted, so a diversion record's bytes predate the cancel.

**Logic.** The crate holds pure validation and merge logic.
- `merge` is a property-tested semilattice.
- `submit` transitions drafts with the clock that the host passes.

**The record envelope** that every writer follows:
- `propose` and `propose_cancel`: rev 1 or bumped.
- `set_retired`: the tombstone and restore.
- `read_document`: a received document in the version this build reads, with its records under the current schema. It drops and counts the unparseable records and any record without exactly one payload.

**The revert's record** (`revert_record`, D-193): a revert of the disruptions log writes each record back as it was before the reverted write, or retires a record that write created. Either is stamped with the next rev and the host's clock, so it wins the merge over a copy a page still holds.
