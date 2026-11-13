# directory

The directory: what exists in the network — the stops (with their walking-group structure; by name, or the places nearest a point), the routes (short and long names), one route's patterns with their ordered stops (list to detail). The pickers and the names join read here; they are MODEL questions, never feed reads. A route runs in directions, and each direction's stop sequences are its patterns (bay and short-turn variants), each with its own ordered stops — what a diversion editor anchors against. The planner's lists ask here too: rows of any kind narrowed by a filter, a route's composition with each pattern trips' trip count and its patterns by direction, a stop's serving routes, where an entity is on the map, and the timetable — one route, one direction, one service combination, across the direction's patterns.

## about

What the network is: its name and the plan it is ({id, name, effective_from}), as the document carries them — the heading a host shows over a network it registered from bytes it never parsed.

## stops

The stop list, name-sorted (the pickers' shape): every stop with its coords, its parent (the flag the station board keys off), and its transfer flag. query narrows it the way a picker's typing does — every whitespace-separated term must be answered, case-insensitive: inside the name or the stop id ("fisher landing" finds Fisher's Landing and its bays); by an abbreviation on the sign — a name word of three letters or more that begins the term, or a word that is a subsequence of the term sharing its first and last letter ("vancouver mall" finds Van Mall, "main street" finds Main St); or within a typo of a name word or its start (one edit — substituted, added, dropped, or swapped letters — from four letters, two from eight, never under four). Exact matches list first, then abbreviations, then typos, each tier by name then id. limit truncates after the sort; omit it for the whole list — a silently truncated directory would be a lie.

## stops_near

The stop list asked from a POINT ({lat, lon}): the places nearest it, by great-circle distance in meters (haversine), nearest first, ties by stop id — each row the stop's id, name, coords, parent and transfer flag (the stops row's fields) plus distance_m. With the point's walks ([{stop_id, walk_secs}] — the streets capability's access answers them, passed as they are), the places a walk reaches come first, by walk (then distance), each with walk_secs: the shortest walk to any of its members; the rest follow by distance with walk_secs null. A PLACE is what a rider walks to: a station's bays answer ONCE, as the station (the walking group the station board departs across), at the distance of its nearest member (the pin or any bay); a stop whose station the network does not hold is its own place. limit caps the rows (default 10); radius_m (meters) drops every place farther than it — omit it for the nearest limit, however far. A lat outside -90..90, a lon outside -180..180, or a negative radius is a request error.

## stops_by_ids

Stops by id, in the request's order — the names join hosts otherwise do by hand when a view (like rt's) carries ids a human reads as names. Unknown ids are omitted.

## routes

The route list, name-sorted: every route with its name (the short public name, else the long one), its long_name ("Lincoln"; null for a route without one), its mode (the model's closed set, not GTFS's numeric codes) and its authored color when there is one.

## route_stop_sequence

One route's patterns with their ordered stops (route_detail groups them by direction). An unknown route answers empty, never an error.

## focus

Where one entity is on the map ({kind: stop | route | pattern | pattern_trips | service_type | block, id}): its lines (each pattern's authored alignment, else its stops joined) and the stops it serves. A stop is its point; pattern trips their pattern; a route, a service type, or a block every pattern it runs. Null for an unknown id.

## route_lines

The network on a map: every route that draws a line, in id order — route_id, name, color (the agency's, or null), and lines (the distinct lines of its patterns: each its authored alignment, else its stops joined — what focus frames for the route). members (service types, optional) keeps only the patterns that run trips under one of them; a route with none is no row.

## service_keys

The service keys, id order: each one's id, name, rule (its calendar as authored — days mon..sun, ranges [{start, end}], exceptions [{date, state}] — what toggle_service_day, the service-range and service-exception edits change), trips (materialized: a frequency trip counts its instances), and routes: [{route_id, name, pattern_trips (the ids under the key on that route — what set_pattern_trips_refs moves to another key), trips}], a route it runs nothing on no row.

## entities

Every entity of one kind ({kind: stop | route | pattern | pattern_trips | service_type | block}), in id order, narrowed by an optional query the way a filter box's typing does (every whitespace-separated term in the id, the name, or a route's long name, case-insensitive — "lin" finds 002 Lincoln). Each row: id, name, context (a pattern's route; pattern trips' pattern · service type), transfer (stops), trips (pattern trips' materialized trip count; a block's chained trips), long_name (routes: the long name beside the short one, null without one).

## route_detail

One route's composition: its patterns, the stops they visit (first-visit order), its pattern trips grouped by service type, each with the trips it runs (the same expansion the materialized day runs), and its patterns grouped by direction (the route's named directions in id order, then any patterns in none). Each direction also lists its VARIANTS, most trips first — the ways a rider rides it: patterns that visit the same places in the same order, a station's bays being one place (so patterns that differ only by the bay they use at a transit center are one variant); each with a name ("first place → last place", and "via" a place only it visits or ", skips" one it does not when another variant shares its ends), its pattern ids, its places (the station or stop, and the stops used there), and its trips. Null for an unknown route.

## stop_detail

One stop's composition: the routes whose patterns visit it (id, name and long_name rows, id order). A stop with service of its own answers its own routes — a bay never answers its siblings'; a stop no pattern visits (a station pin) answers its walking group's. Beside them, the stop's own record (`stop`: name, point, parent, kind, transfer, info) and `visits`: every pattern position that visits THIS stop (never its group's), by route, then pattern id, then position — each its route and label, direction and its name, pattern and name, index, whether it ends the pattern, whether it is a timepoint, the stop before it and the stop after it on the pattern (previous and next — none at the pattern's first and last stop — each its stop, name, the metres along the pattern between them, and published: whether the agency published both distances or the import estimated one), and trips: what the pattern runs under members (the service types asked; every type when omitted), each trip's instances. Null for an unknown stop.

## timetable

The planner's timetable (headed by route_name and route_long_name, null without one): one route, one `direction` (omitted or null: its patterns in no direction), one service COMBINATION by its `members` (analysis service_groups' rows: the types that run together on the same dates, D-55) — every trip its members run on the direction's patterns (service ids key trips, never routes), headed by `group` (the members joined " + ") and `members`, one row per trip by start with its identity (trip, start_secs), its pattern trips and the service key they run under (service_type), pattern and block, its running time end to end (run_secs: the last stop's arrival less its start — each trip's own), and its departure at each column (null where its pattern does not stop); the columns are every stop those patterns visit, in route order, flagged when a timepoint; the legend lists the patterns most trips first, marking headway-band ones. Rows are the materialized day's trips. With `compare_hash` (a loaded baseline) a trip's id is its identity: a one-start trip both run at different starts carries `retimed_from_secs` (its baseline start — kept, never removed and added), a trip the baseline does not run is marked `added`, and `removed` lists the baseline's trips this one no longer runs; a frequency trip's instances share its id, so each is matched by (trip, start_secs). The baseline side runs what the members run there. Null for an unknown route or direction, or members that never run together.
