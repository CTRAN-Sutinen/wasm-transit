# transit-stops

The stops document, beside the plans (D-147, D-153). It depends on transit-network, for the join in `stop_record`, and on transit-requests, for `request_rule`. cap-stops is the seam. A stop's life is its own, managed apart from the picks, which only read it (D-157).

**The rule of a request's reference to its stop** (`request_rule`; D-233, D-238). An open request (drafted, submitted, ordered) needs a stop that is not retired on the write's date. A completed or cancelled request accepts any state. The request that a stop's `retired` event names is exempt, since the retirement is its outcome. `allows` names every state of the stop, so a new state does not compile until the rule judges it, and a law covers every request status. cap-stops' `receive` judges it: refused as of what the writer read, warned as of now.

**A pick reads the stops document** (`stop_record`; D-157, D-228). The join of a network and the stops document is in this crate, because it depends on the network.
- `pick_period` is the period a pick governs: from its own date until the next published pick that is not retired (transit-plans `period_in`). `read_pick` is `read_records` on the pick's first date and `visit_warnings` over that period. `read_records` takes each stop's record as the document has it on a date.
- `visit_warnings` warns of a stop that the pick's patterns visit and that is not in service over the pick's period. The stop can be planned, permitted, built, retired, or not there yet. A suspended stop warns only if it is suspended for the whole period or with no end.

**GTFS out and in carry the stops' history** (D-150, D-151).
- `export_pick` is `export_versions` over the pick's period.
- `export_versions` makes a stop whose record changes inside the export's dates into one stop per version. The last version keeps the id. An earlier version takes `<id>@<date>`. The op also splits each pattern that visits the stop by calendar.
- `import_events` records a feed's differences as events dated the feed's first service date. It reads a made id back as a version.
- `import_feed` does the same, and each event names the feed (`from_feed`, its hash).
- `feed_events` returns the events a feed recorded.
- The stops that the import calls unlisted are only those in service or suspended that day. These are the stops a feed should list.
- The import calls a stop unapplied when it cannot record the feed's difference without breaking the stop's history. A stop retired on the feed's date that the feed lists is always unapplied, even with the same point and details. A retired stop's life has ended, and the document refuses every event after `retired`, so the feed reuses its id. A person checks each unapplied stop by hand on the stops page.
- `seed` begins a stops document from a network's stops.

**The stop.**
- Every stop has a lasting id, kept through a move (D-145).
- Each stop is a list of dated events on the stop's own lifecycle (D-157).
- The lifecycle stages are:
  - planned, permitted, built, and in_service (forward only; a stage may be skipped);
  - suspended (a stop in service that is out of use for a while, back by in_service, or never);
  - moved, changed, and retired (the tombstone).

**What an event sets.**
- The point.
- The details (name, code, station, access, fare zone, and others).
- The amenities (D-154). Each is true, false, or absent for not recorded. The crate uses OpenStreetMap's bus-stop names where one exists and keeps the agency's own.
- The site survey's answers (`site`, D-158). Each question's answer is by name.
  - The core questions (`SITE`) are the Service Stop Worksheet's. The crate checks each answer against its kind: one of a list, a whole number, or text.
  - The agency's own questions are kept and not checked.
- The stop's ids in other systems and its parts' ids (`refs`, D-160: sign, TMS, HP, GIS, shelter, and others, not checked).

**Surveys and attachments.**
- An event that records a survey names its surveyor (`surveyed_by`).
- A stop on a date says when it was last surveyed and by whom.
- Any event carries its photos and drawings (`attachments`, D-162). Each is named by its sha256 content hash with its media type, never a URL.

**Reads.**
- Nothing in the document means today.
- `stop_on` applies a stop's events in order up to a date. A temporary suspended or moved event applies until its `until`.
- `served` asks whether a trip at the stop on a date is served. The time is unread at a day's grain (D-148). Only in service counts as served.
- `on_date` lists every stop that exists.
- `next_events` gives what a stop may become on a date: exactly the lifecycle events that a write would accept.
- `counts_on` gives the stops on a date by status.
- `near` gives the stops on a date nearest a point. It uses great-circle distance, nearest first, ties by id, at most a limit, within a radius where asked, at the statuses asked.

**Writes.**
- Each event has an `id`, unique within its stop (D-166). `identified` gives an id to each event that has none, from the stop and the event's content, so the same event in the same stop gets the same id in any document. Every write identifies before it checks. Another writer's event added above an event does not change which event a correction edits.
- `checked` is what a write must pass. Each refusal names its stop, event, and problem. Two events in one stop with one id are refused.
- `attribute_surveys` names the signed-in writer as the surveyor of every survey that a write adds (the receive path's, D-158).
- A document that passes is canonical as it stands (stops by id, events as written).
- The crate keeps what it does not know through a read and a write, such as another tool's field or an agency's amenity (D-167).
- A correction edits the event that was wrong. A real change adds an event.
- The edits answer the next document:
  - `add_event` places one event by date. `refuse_taken` is the refusal of a write that says a stop is new and names a held id.
  - `correct_event_by_id` takes out the event that was wrong, named by its id, and places the corrected one by date. The corrected event keeps the id.

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### A stop's site survey is typed answers beside its amenities (transit-stops `site`, `SITE`; stops `questions`)

- **Holds:** the site survey is C-TRAN's Service Stop Worksheet, filled at the stop on a phone or tablet. It is part of the stop's details. Each question's answer is by name, set by an event like any detail, and the event names its surveyor.
- **Holds:** a core question takes only an answer of its kind (one of its list, a whole number, or text). The agency's own questions are kept and not checked.
- **Holds:** a part's type (sign, post, shelter, kiosk, light, seat, garbage can) is a written answer. Its id is a reference (`refs`), as are the stop's ids in other systems (Sign ID, TMS, HP, GIS). A type and an id sit on the stop. They are not records of their own.
- **Holds:** the survey never asks what a schedule says (the routes, a time point, the previous and next stop). The survey reads these from a schedule that serves the stop. A new stop's intended routes are its request's.
- **Holds:** the answers stay in this document. GTFS has no place for them, so the export and the import leave them alone, and a survey never splits a stop into versions.
- **Holds:** the page asks `questions` for the questions, so a question added here needs no page change.
- **Holds:** the surveyor is the signed-in person. The receive path names the surveyor on every survey that the held document does not hold (`attribute_surveys`, by the server's configured name claim). It never uses a name that the page sent. So a survey's surveyor is as trustworthy as the sign-in.
- **Because:** many answers are a choice from a list or a type, which yes/no amenities cannot hold, and widening amenities would change a decided shape. A map of named answers keeps the agency's own questions next to the core ones, as amenities do. Signs, posts, and shelters get records of their own only if they move between stops with histories of their own. A v1 reader keeps fields it does not know, so the new fields need no new version.
- **Rejected:** typed values inside `amenities`; a separate survey document (the answers are facts about the stop and change by its events); asset records for each part.
- **Decided:** D-158, D-160, D-163 (D-154 for amenities).

### A feed's stops apply, and the planner sees what they did (transit-stops `stop_record`, cap-stops `import_events`, `export_versions`, `feed_events`)

- **Holds:** when the server imports the agency's feed, each difference between the feed's stops and the stops document on the feed's first service date is recorded as an event dated that day, naming the feed. A stop the feed does not list keeps its events, and events dated later stand.
- **Holds:** the stops page shows what the feed recorded and the stops in service that it does not list. The planner can fix each on its card. A move that was a survey correction becomes a correction of the earlier point.
- **Holds:** on the way out, a stop whose record changes inside an export's dates is one stops.txt row per version, and the rows share its `stop_code`. The version in force last keeps the id. An earlier version takes `<id>@<its first date>`. A re-import reads those back as versions.
- **Because:** the feed is the agency's published statement and riders get it at once. A bad feed is fixed by a correct one, and a planner must see what a feed did. A feed has no dates for its stops, so its first service date is the only date it states. The rider's stop is its `stop_code`. A `stop_id` is for software.
- **Rejected:** a review step before a feed's differences are recorded; ending an export's window at a stop's change (riders could not plan past it); the record in force on the window's first date (wrong after a construction move).
- **Decided:** D-150, D-151.
