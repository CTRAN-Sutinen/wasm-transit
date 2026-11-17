# board

The board: when does service leave this stop? It answers the question in three shapes:

- At a stop. A station pin or a bay with nothing of its own answers its pooled walking group. The answer is realtime-first over the live document that the rt capability mints (its predictions and its service alerts). It also shows what the in-force edits took away from the stop.
- The same stop grouped by route and destination, for the display at it.
- A personal saved set: the watchlist document. This is the same question, parameterized by a personal document.

Every answer is the materialized day over the network document, patched with the in-force edits. The capability is model-native end to end. The domain code lives in the shared transit-dispatch library. Assemble it for anyone who reads a departure board.

## stop_board

The board at one stop on a service date.

The board is the network's materialized day with the in-force edits (`edits_hash`). It is realtime-first when `live_hash` names a registered live document. In that case, the rows are ordered by when the bus actually leaves:

- A late bus that was scheduled before `from_secs` and has not left is still on the board.
- An early bus that already left is not.
- Each row carries `expected_secs`, `delay_secs` (against the board's own patched time), and `status` under the live document's window. Without live data, `status` is `no_prediction`.

A departure that the live document says does not happen is no row. This is a cancelled trip, or a skipped visit at this stop. It leaves the board silently.

Each row has these fields:

- `route_name` and `route_long_name`: the row's route.
- `stop` and `stop_name`: the stop it leaves from. On a pooled board, this is the bay.
- `platform`: the bay's own identifier (GTFS `platform_code`).
- The destination, as the sign says at that visit. This is the visit's headsign, else the trip's, else the final stop's station.
- `wait`: how long until it leaves, from `from_secs` to when it actually leaves.

A trip's final visit is an arrival, never a row.

`wait` has three forms:

- Due, under a minute.
- Minutes, rounded down, under an hour.
- Else the clock time (`secs`).

The answer also has these parts:

- `skipped`: the scheduled departures that the in-force edits took away from the stop. Each has the diversion that skips it, or the cancel that took its trip. A trip that the edit document cancels is never a row. Unlike the live feed's own cancellation, it is listed here. This explains why a stop is emptier than its schedule.
- `alerts`: the live document's service alerts about the board (a row, a skipped visit, or the stop itself). Each row and each skipped visit names the ids that are about it.

A stop with nothing of its own, that belongs to a walking group (a station pin), answers the group's board. Nothing of its own means nothing leaving and nothing skipped. The answer has `pooled: true`, and each row names its bay. A plain stop never pools.

## grouped_board

The board grouped for the display at a stop. It has one group for each route and destination that serves the stop on the date.

A group covers the stop's whole day. A group whose last departure has left stays on the board. It has `departures: []`, which means the route has stopped here today. It also has `next`: when the route runs here next.

`next` is `{date, days_ahead, weekday, departure_secs, stop}`:

- `days_ahead` is 1 for tomorrow.
- `weekday` is a name such as "Monday".
- It is the earliest departure of that route to that destination. It is taken on the first of the next seven days whose day carries one. The day is patched by that date's in-force edits. It never uses the live document.
- It counts only if it leaves at or after `from_secs`.
- It is null when departures are left, or when none runs within the week.

Each group holds its next `per_group` departures (default 3). They follow `stop_board`'s realtime-first rule. They are the same rows, with `wait`, `status`, and the bay.

Each group also has `skipped`: the visits that the in-force edits took from that group. Each has its diversion or its cancel. A cancelled trip takes none of the group's next departures.

Groups list in the route's display order. This is `route_sort_order`, lower first, with routes that have none after them, by id. Then they list by destination. They carry the route's names and authored colors.

`alerts` holds the live document's alerts about the stop, or about any departure that is still to leave it.

A stop with nothing of its own all day, that belongs to a walking group (a station pin), pools the group. The answer has `pooled: true`, and each row names its bay.

An uncut answer has a `per_group` as large as the day. Then the groups' rows together are `stop_board`'s rows.

## watchlist_board

The board for a watchlist: each watched stop's board on a service date. The labels follow the document's version.

This is the same question that the per-stop board answers, parameterized by the personal document. So the answer is the same:

- Each stop's `board` is `stop_board`.
- It uses the same edits (in force, with skipped visits listed).
- It uses the same `live_hash` (realtime-first).
- A watched station pin pools its bays.
