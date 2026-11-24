# transit-codec

The network codec, in both directions, over the whole GTFS Schedule spec.

**The feed layer.**
- `Feed` (src/feed.rs) reads and writes every spec file and column as the exact text the feed carried. It is lossless by construction and writes deterministic zip bytes.
- `import` and `export` map a feed to an authored network document and back.
- The feed layer also answers a feed's summary and its feed_info identity, and base64 for the JSON seam.

**What the import keeps.**
- A feed's trips are the document's trips: the agency's ids, every scheduled time exactly (offsets from the trip's first arrival), frequency trips as frequency trips, and blocks by trip.
- A feed's shapes are the document's alignments: the feed's ids, every point and published distance, including shapes that no trip runs.
- Patterns are stop sequences along an alignment. There is one pattern per (route, direction, stops, exactness flags, shape, and what each stop row says). A stop row says its headsign, pickup / drop-off, continuous stopping, booking rules, and published distance. How long a trip takes is the trip's own.
- A trip with a row that is not a stop at a time (a location group, a zone, a window) is a demand-responsive trip.
- Everything else a feed carries reads into its model home and writes back value for value (src/extras.rs; text and values in src/values.rs).
- A row is exact (the pattern's timepoint) when it carries both times and does not say timepoint=0.

**The `Boundary`** is the import's serialization residue:
- stop_sequence and shape_pt_sequence numbering;
- a frequency trip's stop_times base;
- the id given an agency published without one;
- which file names a route's fare network;
- the services published as dates.

An authored document exports with the default `Boundary`.

**Service ids.**
- Service ids are service types, written back as authored (the calendar row, exceptions as calendar_dates).
- The import keeps every calendar the feed names.
- A service published as dates alone has no weekday flagged and every calendar_dates row an added date. C-TRAN's whole calendar is like this. It imports as the weekly rule its dates read as (D-105).
  - A weekday qualifies when it runs on at least 4 of the dates and on three quarters of that weekday's occurrences in the span (D-111).
  - The import makes one range over the dates on those weekdays, a removed date for each one skipped, and an added date for each other date. The service runs on exactly the feed's dates.
  - When no weekday qualifies, the service stays dated.
- The `Boundary` keeps how the service was written. So the export writes it back as dates: its zero-flag calendar row as written, every running date added. This holds after a fork has carried the service too (tests/dated_calendar_law_test).

**Profiles and layovers.**
- Each pattern gets one all-day timing profile from its earliest trip. This is the production tool.
- Each block layover is the observed gap between consecutive one-start trips of one service type. The planner re-authors from there.

**Directions.**
- GTFS direction_id becomes the route's directions ("0"/"1").
- Each direction is named by the trips' most common headsign, else "To" the commonest terminal.
- The export writes direction_id back and each trip's own headsign. It invents no headsign.

**ImportNotes.**
- The import reports what the model cannot hold yet as an `ImportNote`. It is never silent.
- Examples are stops without coordinates and the trips that serve them, single-stop trips, and a transfer point where fewer than two routes meet.
- A transfer point is the feed's own flag. Nothing infers one, so a feed that never sets the flag counts none (RAPTOR plans transfers regardless).

**The laws.**
- The import is valid by construction over spec-valid feeds (tests/import_prop_test).
- The import keeps the trips (tests/trip_import_law_test).
- `import ∘ export` is a fixpoint (tests/codec_prop_test).
- tests/spec_round_trip_test holds GTFS → network → GTFS against the whole spec, one law per file. The oracle reads the files as raw tables.

transit-codec is a library under cap-network. This is the extraction pattern: crate boundary first, capability wrapper second. The C-TRAN import regressions live in its own test suite (tests/import_regression_test).

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### Dated calendars import as weekly rules (transit-codec calendars)

- **Holds:** a type whose added dates are every flagged weekday of a span, except for a few removed dates, imports as days + range + exceptions. It exports back exactly as the feed had it.
- **Because:** C-TRAN writes all service as dated additions, so nothing is "weekly" and a fork would carry nothing. The rhythm is in the dates. Reading it at the codec keeps the model's rule explicit and the export lossless.
- **Rejected:** inferring the rhythm at the fork; nothing inferred (the planner flags days).
- **Decided:** D-105 (recognition: D-111).
