# cap-stops

The stops capability: what is a stop on a date? transit-stops (with its `stop_record`) is the library. The kernel's `stops` kind registers a document only if `validate` would pass it.

**Reads.**
- `stops_on` gives every stop, or those named or at the statuses asked, as on a date. It gives each stop's status with since, until, and note, and the stops counted by status.
- `stops_near` gives the stops on a date nearest a point, each with its distance. It answers the field's "which stop am I at?".
- `stop` gives one stop on a date: its events, oldest first, and what it may become that day.
- `questions` gives what a site survey asks: the core site questions with the kind of answer each takes, and the core amenity names.

**Writes.**
- `validate` gives what a write must pass: every refusal, or the canonical document and its hash, each event with its id. This is the document that the kernel's `load_stops` answers. It names the fields the document carries that this build does not know (`unknown_fields`, D-167).
- `seed` makes a stops document from a network's stops.
- `add_event` places one edit by date. With `new`, a held stop id is refused (`stop_exists`): a write states the intent "new", and the receive path replays it (D-228).
- `vocabulary` gives the stop statuses, in a stop's order.
- `correct_event` corrects an event that was wrong, named by its id (D-166), or takes it out. With `saw`, the event's hash as the writer read it, an event changed since is a `conflict` that names it, and nothing is corrected.
- Each of the three is checked as a write is.
- `attribute_surveys` names the write's new surveys for the writer. It runs before `validate`.

**What a pick reads.**
- `read_pick` gives the pick with each stop's record on its first date, and the warnings of the stops it visits that are not in service over its period. It derives the period from the published plan set, so no host does. `read_records` and `visit_warnings` answer the same over dates the caller names.
- `records` gives the record that a plan takes of a stop that the document holds and the plan does not, on the pick date (D-173). The plan copies the record. The document is never written.
- `take_stops` gives the `upsert_stop` edits that a new route needs (D-173, D-228): one for each stop the plan does not hold, once each, in order. A stop the document does not answer on the date refuses the take. The host applies the edits.

**GTFS and the stops' history.**
- `export_pick` (over the pick's period), `export_versions` (over named dates), and `import_events` carry GTFS out and in with the stops' history. Amenities, site answers, and references are the document's alone.
- `on` defaults to the feed's first service date. A named `feed` is named on each event recorded.
- `feed_events` gives what a feed recorded.

**The stops log** (D-197).
- `log_records` gives the document as the log's records: one `stop` record for each stop, its events whole, and the document's own record (kind `stops`, id `document`).
- `log_document` assembles the document from those records, registers it, and gives its hash and canonical bytes.
- `receive` writes the stops log whole (D-228): a write's ops over the stops and the stop requests at the head, the surveys attributed and each written document checked, then the entry. One entry can change a stop and the request it carries out, so the module links transit-requests and registers stop requests documents. Its ops are `stops` `add_event`, `correct_event`, `validate` (a whole document), `import_events` (a feed's differences); `requests` `put`, `record_work_order`, `validate`; and `migrate` (an empty log's first entry from the old pointers).
