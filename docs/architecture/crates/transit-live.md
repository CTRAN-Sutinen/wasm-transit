# transit-live

The live document: one rt moment in model vocabulary. It is pure data and has no GTFS layer (arch gate). cap-rt mints it and cap-board reads it. No host computes a delay. It depends on transit-alerts for the alert vocabulary (cause, effect, severity, period).

**Version 2 holds:**
- Delays per trip instance (trip, service date, start) and stop.
  - The date is as the feed states it. An absent date means the current service day, which is GTFS-rt's reading.
  - `LiveDay::on(date, current)` is one date's view. Every reader asks for its own date.
- Vehicles on their trip instances.
- Every status, classified under the on-time window that the document records. There is one window, never a threshold per view. By default any earliness is early, because a bus that leaves early is a missed bus.
- What does not run: a trip instance that the feed cancels (`CANCELED`/`DELETED`: `canceled`) and a stop that it skips (`SKIPPED`: `skipped`). Every reader asks through one rule, `LiveDay::runs`.
- The service alerts in force. Each alert holds one scope for each entity it selects: route (and one of its directions) / stop / trip instance. GTFS-rt joins the fields of one entity with AND, and each entity applies to its own affected entity. All fields absent means the agency. A stop's id is matched as written: a station's id selects no departure at its bays (D-221). `AlertScope::on(date, current)` says whether a trip scope speaks on a date.

The crate also holds the classification, the runs and alert-matching rules, and the status summary.

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### A cancelled trip is not a row (`TripCancel`, `LiveDay::runs`, the board and the router)

- **Holds:** the edit document carries whole-trip cancels beside diversions. A trip that a dispatcher cancels shows on boards as cancelled and is never ridden.
- **Holds:** a trip that the live feed cancels or deletes, or a stop that it skips, is no row on any board, with no mark and no note. The router never boards or alights there.
- **Because:** cancel is a dispatcher's first answer to a late trip, so it lands first. Partial cancel needs a meaning on every reader. Cover, hold, and swap are not edits: they are the dispatcher's day, which CAD/AVL holds (D-212). A live cancellation shown at its scheduled time misled riders. The user chose to drop it silently.
- **Rejected:** the full dispatcher set at once; showing a live cancel marked, or noting it on the group.
- **Decided:** D-45, D-73 (the shape: D-100..D-102).
