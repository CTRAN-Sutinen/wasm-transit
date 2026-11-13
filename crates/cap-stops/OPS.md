# stops

Every stop as a list of dated events, oldest first — planned, permitted, built, in_service, suspended, moved, changed, retired (transit-stops; D-153, D-157). A stop has a life of its own: its stages go forward (planned, permitted, built, in service — a stage may be skipped), a stop in service may be suspended for a while (a closure, works, an event — expected to return, and it may not), and it ends retired. Nothing in the document means "today": a stop on a date is its events applied in order up to that date, and a temporary suspension or move (until) applies only while it lasts, so the earlier state returns on its until date. Every detail — the point, name, code, access, fare zone, amenities (D-154: true there, false checked and not there, absent not recorded) — changes only by an event. A correction edits the event that was wrong; a real change adds one. The stop's id is its lasting key, kept through a move (D-145). A stops document registers on the kernel's load_stops, refused unless validate would pass it. A pick only reads the document (D-157).

## validate

What a write must pass: every refusal, each naming its stop, the event's index in the stop's list, and its problem — no events; an unknown event type; a date that is not YYYY-MM-DD; a missing date (only a first in_service may go undated); events out of date order; a stop not beginning at a stage (planned, permitted, built, in_service); an event after retired; a planned, permitted, or built when the stop is already at or past that stage on its date (stage_back); a suspended when the stop is not in service on its date (not_in_service); an in_service when it already is (already_in_service); an until on or before its event's date; a field its event type does not set (until only on suspended and moved, the point only on a stage event and moved, details only on a stage event and changed, replaces only on a stage event); a missing or half point (required of a stop's first event and of a move); a point off the earth; a consolidation naming the stop itself or a stop the document does not hold. An event out of date order is refused for that alone. With no refusal, answers the canonical document — stops by id, each stop's events as written, every field this repo does not know kept — and its document_hash (the module is the hash authority: the same hash load_stops answers). A document whose schema_version this build does not read is an error.

## stops_on

Every stop as its events make it on the date, in id order — or only stop_ids, where given, and only those at statuses, where given: its status (planned, permitted, built, in_service, suspended, retired), when that status began (since; none for a stop in service from before any record), when it ends (until, a temporary one), the note on the event that set it, its point, and its details and amenities. Beside them, counts: the stops on the date by status (every one, whatever stop_ids and statuses ask), only the statuses some stop has. A stop not yet there on the date (its first event is later) is not listed. Read over a document registered by load_stops.

## stop

One stop on the date — null before it exists — and every event it has, oldest first: the card's record and its history. Beside them, next: what the stop may become that day — the lifecycle events (planned, permitted, built, in_service, suspended, retired) a write dated that day would accept, in that order; a move or a change of details is always a choice while it is not retired. A stop the document does not hold is an error.

## seed

A stops document from a registered network's stops: each stop one in_service event with no date (no feed records when a stop went into service), carrying its point and every detail — registered, answered with its stops_hash.

## add_event

One edit: the document with `event` added to stop `stop_id` in its place by date — after every event dated on or before it (an undated first event first), before every later one, every other event in its order; an id the document does not hold begins a stop. Then checked as a write is (validate): every refusal, each naming its stop, event, and problem, and no document; or none, and the document registered and answered with its stops_hash. A page writes the answer through the deployment's receive path (POST /stops), which checks it again.

## correct_event

One correction: a correction edits the event that was wrong (D-153; the store keeps the earlier version). The document with event `index` (its place in the stop's list, from 0) of stop `stop_id` taken out and `event` placed by date as add_event places one — the same event in its place changes nothing — or, with no `event`, taken out. Then checked as a write is: every refusal and no document, or none and the document registered and answered with its stops_hash. A stop or index the document does not hold is an error.
