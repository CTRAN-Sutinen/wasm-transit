# stops

Every stop as a list of dated events, oldest first. The event types are `planned`, `permitted`, `built`, `in_service`, `suspended`, `moved`, `changed`, and `retired` (transit-stops). A stop has a life of its own. Its stages go forward: planned, permitted, built, in service. A stop may skip a stage. A stop in service may be suspended for a while (a closure, works, or an event). A suspended stop is expected to return, but it may not. A stop ends retired.

Nothing in the document means "today". A stop on a date is its events applied in order up to that date. A temporary suspension or move (one with `until`) applies only while it lasts. The earlier state returns on its `until` date.

Every detail changes only by an event. The details are:

- the point, name, code, access, and fare zone;
- the amenities: true means there, false means checked and not there, and absent means not recorded;
- the answers of the site survey;
- the stop's references to other systems.

A correction edits the event that was wrong. A real change adds an event. The stop's id is its lasting key, and a move keeps it.

Each event has an `id`, unique within its stop. A writer may give one. Every write gives an id to an event that has none. The id comes from the stop and the event's content, so the same event in the same stop gets the same id in any document. A second event with the same content in a stop gets the next free suffix (`-2`, `-3`). An id never changes after that. A correction names its event by this id.

A stops document registers on the kernel's `load_stops`. The kernel refuses it unless `validate` would pass it. A pick only reads the document.

## validate

What must a stops document pass before it is written? `validate` answers every refusal. Each refusal names its stop, the event's index in the stop's list, and its problem.

The refusals are:

- no events;
- an unknown event type;
- a date that is not YYYY-MM-DD;
- a missing date (only a first `in_service` may go undated);
- events out of date order;
- a stop that does not begin at a stage (`planned`, `permitted`, `built`, `in_service`);
- an event after `retired`;
- a `planned`, `permitted`, or `built` when the stop is already at or past that stage on its date (`stage_back`);
- a `suspended` when the stop is not in service on its date (`not_in_service`);
- an `in_service` when the stop already is in service (`already_in_service`);
- an `until` on or before its event's date;
- a field that its event type does not set:
  - `until` only on `suspended` and `moved`;
  - the point only on a stage event and `moved`;
  - details only on a stage event and `changed`;
  - `replaces` only on a stage event;
- a missing or half point (the first event of a stop and every move require a point);
- a point off the earth;
- a `surveyed_by` on an event that sets no details;
- a core site question answered outside its kind (`bad_answer`): an answer not on its list, or a count that is not a whole number (the agency's own questions are not checked);
- a consolidation that names the stop itself, or a stop that the document does not hold;
- two events in one stop with the same `id` (`duplicate_event_id`).

An event out of date order is refused for that alone.

Before it checks, `validate` gives an id to each event that has none. With no refusal, `validate` answers the canonical document and its `document_hash`. The canonical document holds the stops by id and each stop's events as written, each with its id. It keeps every field that this repo does not know. The module is the hash authority: the hash is the same one that `load_stops` answers.

A document whose `schema_version` this build does not read is an error.

## stops_on

Which stops exist on a date, and in what state? `stops_on` answers every stop as its events make it on the date, in id order. It answers only the `stop_ids` where given, and only the stops at `statuses` where given.

Each stop answers these fields:

- its status (`planned`, `permitted`, `built`, `in_service`, `suspended`, `retired`);
- `since`: when that status began (none for a stop in service from before any record);
- `until`: when the status ends, for a temporary status. It is the event's `until`, or sooner the next status event after the date (a stop back early, or retired while suspended);
- the note on the event that set the status;
- `surveyed_on` and `surveyed_by`: when the stop was last surveyed and by whom;
- its point;
- its details, amenities, site answers, and references.

Beside the stops, `stops_on` answers counts: the stops on the date by status. The counts cover every stop, whatever `stop_ids` and `statuses` ask. They list only the statuses that some stop has.

A stop not yet there on the date (its first event is later) is not listed. `stops_on` reads over a document that `load_stops` registered.

## stops_near

Which stops are nearest a point on a date? This is the field's question "which stop am I standing at?", asked from the device's location.

`stops_near` answers every stop that exists that day, as `stops_on` answers it. Only those at `statuses` are answered where given. A planned stop that is not yet in any schedule is one of them. Each row also has `distance_m`: the great-circle distance in metres from {lat, lon}. The distance uses the haversine formula with the mean earth radius. The directory's `stops_near` measures the same way. The rows come nearest first, with ties by stop id.

The request fields are:

- `limit`: caps the rows (default 10).
- `radius_m`: drops every stop farther than it.

A `lat` outside -90..90, a `lon` outside -180..180, or a negative radius is a request error. `stops_near` reads over a document that `load_stops` registered.

## stop

What is one stop's record on a date, and its history? `stop` answers the stop on the date (null before it exists) and every event it has, oldest first. These are the card's record and its history.

It also answers `next`: what the stop may become that day. `next` lists the lifecycle events (`planned`, `permitted`, `built`, `in_service`, `suspended`, `retired`) that a write dated that day would accept, in that order. A move or a change of details is always a choice while the stop is not retired.

A stop that the document does not hold is an error.

## seed

What stops document do a registered network's stops make? `seed` answers a stops document with one `in_service` event for each stop, with no date (no feed records when a stop went into service). Each event carries the stop's point and every detail. The document is registered and answered with its `stops_hash`.

## add_event

What is the document after one more event? `add_event` adds `event` to stop `stop_id` in its place by date:

- It goes after every event dated on or before it. An undated first event goes first.
- It goes before every later event.
- Every other event keeps its order.
- An id that the document does not hold begins a stop.

With `new` true, the write says the stop is new. An id that the document holds is then refused (`stop_exists`), and nothing is added. Without `new`, an event added to a held stop is an ordinary write.

Then `add_event` checks the result as a write is checked (`validate`). It answers one of two things:

- every refusal, each naming its stop, event, and problem, and no document;
- no refusal, and the document registered and answered with its `stops_hash`.

A page writes the answer through the deployment's receive path (POST /stops), which checks it again.

## correct_event

What is the document after one correction? A correction edits the event that was wrong (the store keeps the earlier version).

`correct_event` takes out the event `event_id` of stop `stop_id`. Another writer's event added above it does not change which event it edits. Then it does one of two things:

- With `event`: it places `event` by date, as `add_event` places one. The corrected event keeps the id. The same event in its place changes nothing.
- With no `event`: it only takes the event out.

Then it checks the result as a write is checked. It answers one of two things:

- every refusal and no document;
- no refusal, and the document registered and answered with its `stops_hash`.

A stop or an event id that the document does not hold is an error.

With `saw`, the canonical hash of the event as the writer read it, the correction runs only over that event. When the document holds the event changed since, the answer is `conflict` with the event's id, and nothing is corrected (D-166). So two writers who change different events both land, and two who change one event are told which.

## vocabulary

The lists that a stop's answers come from. The op reads no document.

- statuses: `planned`, `permitted`, `built`, `in_service`, `suspended`, `retired`, in the order a stop goes through them.

## read_records

What does a pick read of the stops? `read_records` answers the registered network with each stop's record, as the stops document has it on `on`, the pick's first date.

A stop's record is its point, name, code, station, and details. The amenities, site answers, and references belong to the stops document alone.

A stop that the document does not hold, or that is not there yet that day, keeps the network's own record. Nothing else in the network changes. The stops document is never written. A network read from a document seeded from it comes back under the same hash. `read_records` registers the network and answers it with its `network_hash`.

## records

What does a plan take of a stop that the stops document holds? The planner that adds a stop to a pattern may pick one that the stops app made. The plan copies its record.

`records` answers each of `stop_ids` that is there on `on` (the pick date), keyed by stop id. Its record is in the network's terms. It is the same record that `read_records` gives a pick: point, name, code, station, and details. The amenities, site answers, and references stay the stops document's.

A stop that is not there yet that day, or that the document does not hold, is not answered.

The plan takes a record with author `upsert_stop`. The stops document is never written. `records` reads no network.

## take_stops

Which edits take stops into a plan? A new route may visit stops that the stops document holds and the plan does not. `take_stops` answers `edits`: one `upsert_stop` for each of `stop_ids` that the plan (`network_hash`) does not hold, once each, in the order named. Each carries the record that `records` answers on `on`.

A stop that the plan holds is left as the plan has it. If any stop is not answered on `on` (not there yet, or not held), the op is refused and names each one. The host applies the edits with author `apply`. The stops document is never written.

## read_pick

What does a pick read of the stops, over the dates it governs? `read_pick` answers the pick's `period`, the pick with each stop's record on its first date, and the warnings of the stops it visits.

The pick's period is from its own date until the date of the next pick in `published` that is not retired. `published` is the published plan set, as the server holds it (none before the first publish). The pick is placed in the set by its id, so the date it carries wins over the date the set holds for it. A retired pick bounds nothing. The last pick is open-ended: its `period` has no `until`.

`network` and `network_hash` are what `read_records` answers on the period's first date. `warnings` are what `visit_warnings` answers over the period. A pick with no id, or with no date, is not a pick: `period` is absent, the network comes back as it was, and there are no warnings. Nothing is written to the stops document.

Every host asks this op. None derives the period itself.

## visit_warnings

What is the planner told of the stops that the network's patterns visit over its period? The period runs from `from` until `until` (exclusive; open-ended without `until`).

`visit_warnings` reports two kinds of span:

- each span in which a stop is not in service: planned, permitted, built, retired, or not there yet (no status);
- each span in which a stop is suspended, when the suspension covers the whole period or has no end. A shorter suspension is real time's to say, not the pick's.

Each warning is `stop_not_in_service` with the stop, its status, and the span, in stop order. These are warnings and never refusals. A stop that the document does not hold says nothing.

## export_versions

What is the stops' history in a GTFS export? `export_versions` answers the registered network over its dates, from `from` until `until` (exclusive; open-ended without `until`), for cap-network's export.

A stop whose record changes inside the dates becomes one stop for each version. The record is its point, name, code, and details. The ids of the versions are:

- The version in force on the last date keeps the stop's id.
- Each earlier version gets `<id>@<its first date>`. The first version's date is `from`.
- Each version has its own record. The code that the stop keeps is shared.

Each pattern that visits such a stop splits at those dates:

- For each earlier version's dates, there is a copy of the pattern (`<pattern>@<date>`) and a copy of its trips (`<trip>@<date>`). They run under the service type cut to those dates (`<type>@<first date>..<last date>`).
- The last dates keep the pattern's and trips' own ids. They run under the type cut from their first date (`<type>@<first date>`).
- Two patterns that split at different dates never share a cut type.
- Each block runs a trip's copies beside it. The copies never share a day.

On every date, the export runs exactly the trips that the pick runs that day, at the stops' records of that day. Before `from`, the first version runs. From `until` on, the last version runs.

A suspension or a retirement changes no record, so it splits nothing. With no record changing, the answer is `read_records` on `from`.

`export_versions` registers the network and answers it with its `network_hash`.

## export_pick

What are the stops' history in a pick's GTFS export? `export_pick` answers what `export_versions` answers, over the dates the pick governs: from its own date until the date of the next pick in `published` that is not retired. The period is derived as `read_pick` derives it.

A pick with no id, or with no date, comes back as it was. The answer is registered and answered with its `network_hash`, for cap-network's export.

## import_events

What does a GTFS import change in the stops' history? `import_events` compares a feed's stops with the stops document on `on`. The feed's stops are the network that its import answered. `on` is the feed's first service date. When `on` is omitted, it is the feed's own first service date (the earliest date that any of its service runs), and the answer gives it as `on`.

Each difference becomes an event dated that day. When `feed` (the feed's id, its hash) is given, each event names the feed (`from_feed`). The events are:

- `in_service`, with the feed's point and details, for a stop that the document lacks or that is not yet in service (planned, permitted, built, or not there yet).
- `moved`, for another point.
- `changed`, for the details that differ. A detail that the feed does not carry is no difference. The amenities, site answers, and references are the document's alone.

A suspended stop that the feed lists stays suspended: a suspension is real time's. Every event already recorded stands.

A made id (`<id>@<date>`, from `export_versions`) is a version of its stop. `import_events` compares the version in force on `on`. That is the made version with the latest date on or before `on`, else the stop's own id. A feed does not say where its own id's version begins, so `on` must be the feed's first service date. So a re-import of this repo's own export adds no event.

The answer has these parts:

- the next document, registered, with its `stops_hash`;
- `added`: each event added, as a stop and an event;
- `unlisted`: the stops that the document has in service or suspended on `on` and that the feed does not list. They keep their events. A stop that is planned, permitted, built, retired, or not there yet is expected to be missing from a feed, and `unlisted` does not name it;
- `unapplied`: the stops whose difference would break their history. Nothing is added for them.

The caller writes the document through the receive path.

## questions

What does a site survey ask? `questions` answers the core site questions, in the order that C-TRAN's Service Stop Worksheet asks them. Each question has the kind of answer it takes:

- `one_of`: its answers are listed;
- `number`: a whole number;
- `text`.

`questions` also answers the core amenity names.

An answer is recorded in an event's site, by question. A part's id goes in its refs (sign, tms, hp, gis, shelter, and so on). Who surveyed goes in its `surveyed_by`. The agency's own questions are kept and not checked. `questions` reads no document.

## attribute_surveys

Who surveyed? The surveyor is the signed-in writer, as the receive path takes a write.

`attribute_surveys` takes `stops`. It finds every survey event in it: an event that carries `surveyed_by`, either a name or empty to leave it to the writer. It compares each one, whole and in the same stop, with the held document (`stops_hash`, registered by `load_stops`; absent for a first write). It names `by` on each survey event that the held document does not hold. This is true whatever name the client sent, and it includes a held survey that the client rewrote.

Every event that the held document holds keeps its surveyor. Nothing else changes. Attributing twice is attributing once.

`attribute_surveys` answers the document unchecked: `validate` follows. POST /stops runs it with `by` set to the configured claim of the verified token (`-oidc-name-claim`).

## feed_events

What does the planner see of an import? `feed_events` answers the events that the document holds and that the feed `feed` recorded. `feed` is the feed's id, the hash that `import_events` named. Each event comes with its stop. The rows are in stop order, and each stop's events are in their order. A feed that recorded nothing answers none.

## log_records

What does the stops log hold of this document? `log_records` answers the stops document `stops_hash` as records of kind `stop`, one for each stop, by its lasting id, with its events whole and in their order (D-197). The answer is `next`: those records, and the document's own record, of kind `stops` and id `document`: the document with its collection empty (a valid document with no records), so a field outside the collection that this build does not know is kept (D-167). Each names the document's version. The log capability composes an entry from it.

## log_document

What is the stops document that the stops log holds? `log_document` assembles the document from the log's `stop` records and its own record `document` (each `{version, value}`, by stop id), registers it, and answers its `stops_hash` and its canonical `document`, which a host serves under that hash. A record in another version is refused, naming both versions: an older record comes forward through the migration capability as a one-record document.
