# cap-router

The router capability answers one question: how do I get from here to
there? It reads the network document and the effective day. The
effective day is the model's materialized trips, patched with the edits
in force. Every day view starts from the same transit-dispatch
composition.

- When `live_hash` names a live document (cap-rt's live answer), the
  router reads the day at that document's predictions.
- When `walks_hash` names a walk table (cap-streets' `walks` answer),
  transfers walk the streets. The router refuses a walk table derived
  for another network. With no walk table, transfers are straight
  lines.
- The RAPTOR graph is a derived entry for each (network, edits, live,
  date, walks). Only the graphs for the newest live document stay.

A plan's `from` and `to` are stop ids or places (`{id, name, walks}`).
A place passes cap-streets' `access` walks as they are. `direct` is the
walk between the two ends (from `access`).

- `plan` leaves at a time.
- `plan_arrive_by` answers the latest departure that arrives by a
  deadline. With no deadline, it answers the last bus home.
- `itinerary_paths` takes the plan's ends and `direct`. It draws a
  place's walks along their street paths (`transit_router::walk_path`).
  Without them, a place draws nothing.

The compare facet is the diversion delta: the unpatched day read at the
same predictions, one materialization apart, with its notice. The
`itinerary_paths` facet draws a plan's legs over the same day: a
transit leg's ridden segment, and a walk through its via stops. The
browser plans only through `plan_compare`, with no edits hash, so there
is no baseline. The browser shows the notice's text.

cap-router is a pure-function capability. It writes nothing.
