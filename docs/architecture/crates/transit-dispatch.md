# transit-dispatch

The materialized day's views, model-native. It is a library shared by the cap-board, cap-runcut, cap-calendar, and cap-divert modules. The UI reads them here. It holds no GTFS layer (the justfile arch gate enforces this).

**Patching the day.**
- `patch_day` skips or shifts materialized stop times. It acts on the day, never the calendar.
- `cancel_day` takes a cancelled trip instance out of the day. Every other trip stays whole.
- Edit validation runs against the network: a cancel's trip must exist and run the instance it names.

**The board** (`stop_board`).
- A stop with nothing of its own pools its walking group.
- The board is realtime-first over the live document. Rows are ordered by when the bus actually leaves. A visit that the document cancels or skips is no row (D-73).
- Each row has:
  - its expected time, delay, status, and wait (`wait`, the one rule: due, then minutes rounded down under an hour, then the clock time);
  - its route's short and long names;
  - its bay's name and platform code;
  - its destination as the sign says there.
- `skipped` lists the departures that the day's edits took away. Each entry has the diversion that skips it or the cancel that took its trip. A dispatcher's cancel is listed, unlike the live document's (D-100).
- The `Day` holds the unpatched day beside the patched one. It is one materialization: `day` and its key, which the kernel holds.
- The live document's alerts about the board are included, each row naming its own.

**The grouped board** (`grouped_board`).
- It groups the board's rows by route and destination for a stop's display.
- It shows every group that serves the stop all day. A group that has stopped stays, with no departures.
- Each group has its next `per_group` departures and its skipped visits.
- Order is route_sort_order, then id, then destination.
- `with_next_service` gives a stopped group its `next`. This is the first departure of the next seven days, whose patched day carries it (D-42).
- Each day is asked through a closure, so the capability decides where the days come from, never the live document.

**Other day views.**
- The day's blocks list.
- The fleet summary: peak concurrent vehicles and when, platform / revenue / layover hours.
- The service keys that run on a date.
- The authored block chain with the layover ripple. `layover_remaining_secs` goes negative when a connection is eaten.

**The diversion editor** over the network (divert.rs).
- A draft's view has:
  - the route's patterns, with the longest as the default;
  - each visit's role, with the interior skipped unless served;
  - the alignment;
  - the flattened trace and its length;
  - the problems.
- The record that a ready draft becomes.
- The trip cancel that a pick becomes (`cancel_of`: the trip, one instance or all, on a date its service runs).
- The edit document's named rows: diversion rows and cancel rows.

**An alert's reach** (`alert_reach`, `alert_reach_day`). The departures a board shows an alert on, by the board's own rule: a scope selects a departure when every field it names matches, or when it names the departure's stop and no route or trip. A trip scope speaks on its date only. Over a held day it also reaches the departures the edits took away, which the board lists as skipped (tests/alert_reach_law_test).

**The board at a moment** (`stop_board_at`, `grouped_board_at`).
- The op gives the date's board. Given the previous date at the same instant, it also gives that date's departures that are still to leave.
- The instant is the clock's seconds and the date's length (23, 24, or 25 h).
- GTFS times past 24:00 are the date the trip started on. At 00:30 Friday, Thursday's 24:45 leaves in 15 minutes.
- Each date reads its own edits and its own live instances. Every row names its date.
- Rows order by how long until each leaves.
- In the hour before a fall-back day's anchor, the wait counts from the true instant, not the clamped 0.

**Where service runs today** (geometry.rs).
- A trip's line over the effective day is the authored alignment.
- For a diverted trip, the line is the alignment to the first kept stop, then the traced detour, then the alignment from the last kept stop. A skip-only diversion keeps the alignment.
- The ridden segment is that line sliced between two stops.

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### A stopped route looks ahead up to a week (board `grouped_board`, transit-dispatch next service)

- **Holds:** a group with no departures left shows the next departure from the following days. Each day is patched by its own edits and never by the live feed. Nothing shows past seven days.
- **Because:** "Next: Monday 5:12 am" is what a rider at a stopped route needs. One weekly cycle covers every regular pattern.
- **Rejected:** tomorrow only; until the calendar ends.
- **Decided:** D-42.
