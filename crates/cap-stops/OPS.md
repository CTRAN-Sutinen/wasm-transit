# stops

Every stop as a list of dated events, oldest first — opened, closed, reopened, moved, changed, proposed, retired (transit-stops; D-153). Nothing in the document means "today": a stop on a date is its events applied in order up to that date, and a temporary closure or move (until) applies only while it lasts, so the earlier state returns on its until date. Every detail — the point, name, code, access, fare zone, amenities (D-154: true there, false checked and not there, absent not recorded) — changes only by an event. A correction edits the event that was wrong; a real change adds one. The stop's id is its lasting key, kept through a move (D-145). A stops document registers on the kernel's load_stops, refused unless validate would pass it. The network document keeps its shape: a pick's stops are a copy, refreshed from the stops document for the pick's window (D-156).

## validate

What a write must pass: every refusal, each naming its stop, the event's index in the stop's list, and its problem — no events; an unknown event type; a date that is not YYYY-MM-DD; a missing date (only a first opened may go undated); events out of date order; a stop not beginning opened or proposed, or a proposal anywhere but first; an event after retired; a reopened when the stop is not closed on its date; a proposed naming no plan; an until on or before its event's date; a field its event type does not set (until only on closed and moved, plan only on proposed, the point only on opened, proposed and moved, details only on opened, proposed and changed, replaces only on opened and proposed); a missing or half point (required of a stop's first event and of a move); a point off the earth; a consolidation naming the stop itself or a stop the document does not hold. With no refusal, answers the canonical document — stops by id, each stop's events as written, every field this repo does not know kept — and its document_hash (the module is the hash authority: the same hash load_stops answers). A document whose schema_version this build does not read is an error.

## stops_on

Every stop as its events make it on the date, in id order — or only stop_ids, where given: its status (proposed, open, closed, retired), its point, the plan that proposed it, and its details and amenities. A stop not yet opened or proposed on the date is not listed. Read over a document registered by load_stops.

## stop

One stop on the date — null before it exists — and every event it has, oldest first: the card's record and its history. A stop the document does not hold is an error.

## seed

A stops document from a registered network's stops: each stop one opened event with no date (no feed records a stop's opening), carrying its point and every detail — registered, answered with its stops_hash. Refreshing the network from it gives the same network back, under the same hash.

## refresh

The network with its stops refreshed from the stops document for the window from–until (until exclusive; open-ended without it), for the pick's plan — the copy never wins. Each stop the document holds is the pick's as the window opens (or as it first exists inside it); unless it is open as recorded on every day of the window, it carries the events that apply inside it (a closure, a move, a rename, an opening, a retirement), so a day reads it on its dates. A stop is in the network when it exists on a date of the window other than retired, or when the network already has it; another plan's proposals stay out, the plan's own are in as proposed; a stop the document does not hold is left as it was. A network whose stops do not change keeps its bytes and hash. Registered, answered with the network and its network_hash.
