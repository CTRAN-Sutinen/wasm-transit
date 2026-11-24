# cap-board

The board capability answers one question: when does service leave
this stop? It answers at a stop, at a station's pooled bays, or for a
personal saved set. The saved set is the watchlist document, version 1
or 2, read in either form (cap-migrate migrates it). A personal board
of many stops is the same question with a personal document as its
parameter.

cap-board reads the kernel's network, edit document, live document
(cap-rt's, by hash), and watchlist. Its ops:

- `stop_board` is the board. The module answers all of these, never the
  host: pooling, realtime-first order, the row status, the route's long
  name, the bay's name, the sign's destination, the visits that the
  in-force edits skip and the diversion that skips each, and the live
  alerts about the board.
- `grouped_board` gives the same rows by route and destination. By
  default it gives the next three of each. This is the stop display's
  answer. A group that has stopped says when it runs next within a
  week. It materializes the later days itself and does not hold them.
  The kernel holds only a few days, and a week of days would push them
  out.
- `alert_reach` gives what one service alert reaches on a date, entity
  by entity: each informed entity as the GTFS-rt feed publishes it,
  what it names, and the departures a board shows the alert on.
- `watchlist_board` gives each watched stop's `stop_board`, with the same
  edits and live document.

Each op takes `previous`: the previous date at the same instant, from
the host's clock. With it, the op also answers that date's departures
after midnight that have not left yet. Each such row names its date
(`stop_board_at`, `grouped_board_at`). A trip's final visit is an
arrival and is never a row.

The board ops read one materialized day: `materialize(net, date)`,
patched with the in-force edits. This is the kernel's held day: one
materialization for each (network, edits, date), however many boards
ask. There is no GTFS expansion. The domain code is in the shared
transit-dispatch library. The arch gate forbids a GTFS layer here. The
UI's departure board renders `stop_board`'s answer. Each row carries the
address of its trip instance (trip and start, and its date when the
board is asked at a moment). So the click-through needs no join.
