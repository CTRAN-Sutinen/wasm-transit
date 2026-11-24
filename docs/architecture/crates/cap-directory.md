# cap-directory

The directory capability answers one question: what exists in the
network? Its ops:

- `about`: what the network is, its name and plan. This is the heading
  of a page that holds the network only as bytes.
- `stops`: the stop list, sorted by name, with the walking-group
  structure the station board keys on. An optional query narrows the
  list the way typing narrows a picker. The match is fuzzy: exact
  matches first, then a sign's abbreviations, then typos (D-48).
- `stops_by_ids`: the names join.
- `stops_near`: the places nearest a point. Distance is in great-circle
  meters, nearest first, with ties broken by id. It answers at most
  `limit` places, and only within `radius_m` when it is given. A
  station's bays answer once, as the station, at the distance of its
  nearest member. This is the walking group that the board pools. When
  the point's `walks` are given (cap-streets `access`), the places a
  walk reaches come first, by walk (the shortest to any member). The
  rest follow by distance (D-34). Names are matched by the sign's own
  words, never by an alias.
- `routes`: the route list, in the model's closed set of modes. Every
  row that names a route carries its long name. This is true for
  `routes`, `entities`, `stop_detail`'s routes, and the timetable
  header.
- `route_stop_sequence`: one route's patterns, with their ordered
  stops. The diversion editor
  anchors against these. New edit records carry `direction_id` null,
  which is the documented case for both directions.
- `focus`: where an entity is on the map. The planner's drill-down
  frames it.
- `entities`, `route_detail`, and `stop_detail`: the planner's lists.
  `entities` gives filtered rows for each kind. The forms' pickers and
  the document counts read them too. `route_detail` gives a route's
  composition with trip counts, and its patterns and rider variants by
  direction. `stop_detail` gives a stop's serving routes: its own, or a
  station pin's bays' (the board's rule). It also gives the stop's own
  record, and every pattern position that visits it, with the trips
  each runs under the members asked.
- `timetable`: the planner's timetable, for a route, a direction, and a
  service type, across the direction's patterns.

The pickers are model questions, never feed reads. The UI's comboboxes
and the map's name lookups read this module. The feed line reads the
kernel's `load_feed` summary and cap-gtfs's `feed_info`. The planner
computes no number.
