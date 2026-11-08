# Architecture — the authoritative reference

This is the ONE authoritative architecture doc. The compact tree
and ownership rules live in the root `AGENTS.md`; per-crate,
server, web, and bus detail lives here and nowhere else. The
three architecture laws (the network is the core data model;
nobody reinvents the monolith inside a module; one NEED per
module, personas assemble at wire time) are spelled out in
`crates/AGENTS.md` — that file and this one are updated together.
This doc describes the tree as it is; open work lives in
`docs/units/`, one file per unit.

## Layering (bottom up)

```
transit-network The authored network model (planner authority):
                routes and their DIRECTIONS (`Route.directions`,
                each named; a pattern runs in at most one —
                `Pattern.direction`, validated against its route;
                bay and short-turn variants share one),
                patterns, timing profiles, service types (the
                service identifier, linked to days by its calendar
                rule), pattern trips (`PatternTrips`: one pattern's
                trips under one service type — NOT a planner's
                schedule; the timetable a planner reads is a route's
                direction under one service COMBINATION, across its
                patterns), runcut. A service COMBINATION
                (`service_combinations`, D-55) is a set of service
                types that run together on the same dates, named by
                its members joined " + " and shown with the dates it
                holds on (folded into spans: a new span where the next
                date is more than seven days on, D-94): every date
                with service is in exactly one, whose members are
                exactly the types with trips that run that date, so
                types that never share a day are never pooled and a
                dated exception is a combination of its own
                (tests/combination_law_test). A TRIP is a stored record: its
                own id, its own departure (`Departs::At` one start,
                or `Departs::Every` a frequency — one instance per
                band slot, sharing the trip's id and times), and its
                own time at every pattern stop (`StopOffset`s from
                its start; a timepoint stop carries both times, an
                unscheduled one materializes by distance between its
                scheduled neighbours). A pattern's timing profiles
                are the planner's tool for PRODUCING a new trip's
                times, never the schedule. A block references trips
                by id and runs each whole (a frequency trip's
                instances back to back); it may hold service types
                that never share a day, and is judged day by day.
                A wait between two adjacent trips of a block is
                judged against the recovery REQUIRED there (D-9,
                `required_recovery` — the one rule every reader
                applies): a work rule's requirement when the caller
                holds one (`validation_issues_under`; it wins), else,
                for a pair carrying an authored layover, the smallest
                authored layover at that terminal (`recovery_floors`;
                the layover's `at`, else the stop the trip before ends
                at). A layover's own authored length is slack, never
                the requirement: an imported layover is the whole
                observed gap, and a later arrival spends slack.
                An ALIGNMENT is a line on the ground, its own record
                (`Network::alignments`: its id, its points, the
                distances published along it); a pattern runs one by
                reference (`Pattern::alignment`), so a pattern is its
                stops along its line. Patterns may share one, and one
                no pattern runs still stands.
                `materialize` answers a day's instances of the stored
                trips. The document leads with `"kind": "network"`
                and `"schema_version"` (2); its chain (`version.rs`,
                kept forever, wrapped by cap-migrate) brings v1 — any
                untagged document — forward, including those written
                before trips were stored (each old departure becomes
                a trip `{pattern_trips}@{start}`) or before
                alignments stood alone (a pattern's inline `path`
                becomes an alignment under the pattern's id).
                `trip_instance` answers one instance date-free: a
                one-start trip at its start, an exact frequency trip
                on its grid, a non-exact one at any start inside a
                band. A trip instance is addressed (trip, start)
                everywhere. WHERE A BUS IS GOING is one rule
                (`signs.rs`, `visit_destination`): the visit's own
                headsign (it overrides the trip's at that visit
                only, as GTFS's stop_headsign does), else the trip's,
                else the final stop named as its place (the parent
                station, never the bay) — every board row and
                itinerary leg reads it. The network models everything the GTFS
                Schedule spec describes, in model vocabulary, each
                family in its own module: per pattern stop its
                headsign, pickup / drop-off and continuous-stopping
                rules (`StopRule`), booking rules, and whether its
                distance was published; per trip its headsign,
                public number, and wheelchair / bike / car
                accommodation; per route its long name, description,
                URL, text color, sort order, continuous stopping,
                operator, fare network, contactless acceptance, and
                a published vehicle type where the mode does not say
                it (the short name is `Route::name`, possibly empty;
                display by `Route::label`); per stop its kind (stop
                or station) and `StopInfo` (code, spoken name,
                description, URL, fare zone, wheelchair boarding,
                platform, level, timezone, street access).
                `operators.rs`: operators, the publisher,
                attributions, translations. `places.rs`: station
                parts (entrances, nodes, boarding areas), levels,
                pathways, transfer rules. `fares.rs`: zone fares and
                fare products (areas, fare networks, timeframes,
                rider categories, media, offers, leg / join /
                transfer rules). `demand.rs`: demand-responsive
                service — location groups, zones (a GeoJSON
                FeatureCollection), booking rules, and trips that
                visit stops, groups, or zones within windows. GTFS is a codec at
                the boundary, not the model; no GTFS types leak in
                here. Transfer points are authored flags on
                stops, composed over their WALKING GROUP (the stop +
                siblings sharing a parent, the parent, children —
                transfers happen across a street, not at one pin).
                Every document is a SERVICE PLAN (`PlanMeta`: id,
                name, effective_from) — agencies implement a few
                picks a year at defined dates; empty identity =
                unversioned (a fresh import). AUTHORING lives here
                too (`edit.rs`): typed, atomic `NetworkEdit`s
                answer the NEXT document (never mutating the last);
                removals refuse while referenced. Two levels:
                entity upserts/removes, and GESTURES — one edit per
                thing a planner does, each carrying its coherence
                rule (timing aligned with timepoints, every trip's
                times aligned with its pattern's stops, layovers only
                between trips, ids minted and reported, the plan
                frame merged field-wise; a fork given its period end
                carries every weekly calendar still running in the
                calendar's final week to it while dated service and
                closed periods stay as they are, D-41, D-112; a redrawn line names
                whether it moves one pattern or every pattern on its
                alignment and reports the patterns it touched, D-11) —
                and the TIMETABLE's gestures (`timetable_edit.rs`:
                headway, span, running time, and stops, stated
                for a route + direction (+ service type) and
                applied across its patterns; a trip that stays is
                retimed and keeps its id, so its blocks keep it; one
                that goes leaves them; a new one gets a fresh id and
                is unassigned; a running-time change names its
                service type). Validation answers issues (gate: a
                block naming a trip the network does not have is
                one) and WARNINGS that never gate
                (unassigned trips; a pick with no service in its
                first week). GEOMETRY lives here too
                (`geometry.rs`): a pattern's line (the alignment it
                runs, else its stops joined — the model's own
                contract) and where an entity is (`entity_focus`: a
                stop's point; the lines and stops of every pattern a
                route, pattern trips, service type, or block runs). The
                planner's LISTS live here too (`listing.rs`): rows
                of any kind narrowed by every filter term (id, name,
                and a route's long name), the stops a rider's typing
                finds (`find_stops` / `name_match`, D-48: exact, then
                an abbreviation on the sign — "Van" for "vancouver" —
                then a typo; the thresholds D-91), the places near a
                point (`stops_near`; `stops_near_walked` ranks them by
                the point's walks), and a route's composition
                (patterns, the stops they visit, pattern trips by
                service type, patterns by direction, and each
                direction's VARIANTS — the ways a rider rides it:
                patterns over the same places in order, a station's
                bays one place, named by their ends and by what sets
                them apart, most trips first) — each entry's trip
                count is its trips' instances, the materialized
                day's own expansion — a stop's serving routes, and the
                planner's TIMETABLE (`timetable`: one route, one
                direction, one service combination — every trip its
                members run on the direction's patterns by start, one column per stop
                in route order, a variant's stops slotted where it
                leaves the others; rows are `materialize_service`'s
                trips, never a second derivation, each with its own
                end-to-end running time, `run_secs`).
transit-router  RAPTOR planning over the network model's
                materialized day; pure (no I/O, clock, or calendar),
                GTFS-free. `RouterNetwork::build(network, day, date,
                live)` groups the caller's day (plain or effective)
                into patterns by actual stop sequence — an edited
                trip rides its own pattern. With a live document,
                each trip instance rides at its predicted times where
                the document states them (delays against the
                published schedule, clamped so a trip never runs
                backwards), the day's own times elsewhere; a trip it
                cancels is not in the world planned over, and a stop
                it skips is not a stop of the trip (`LiveDay::runs`).
                A plan's ends (`End`) are stops or PLACES off the
                network (`Place`: an id never a stop's, a name, its
                walks to the stops near it): from a place the rounds
                start at every stop it walks to, each at its walk's
                time, and to one they end at the best of its stops
                plus its walk — or at the direct walk between the
                ends, which wins ties (D-33). The itinerary and its
                first and last walks name the place.
                `earliest_arrival_between` answers leave-at;
                `latest_departure_between` answers ARRIVE-BY — the
                same rounds run backward from the destination, and
                with no deadline the service day's last arrival: the
                last bus home; its itinerary is the forward plan from
                that departure (`earliest_arrival` /
                `latest_departure` are the stop-to-stop forms). A
                transit leg names its trip instance,
                the route's label and long name, the sign at the
                boarding visit (`visit_destination`), and how far
                each predicted time moved (board/alight_delay_secs,
                null without a prediction). An itinerary is door to
                door (D-40): `leave_secs` (the first boarding minus the
                walk before it; with no ride, `depart_secs`),
                `duration_secs` (arrival minus leave), and `leave` — the
                board's one wait rule (`transit_dispatch::wait`) from
                `depart_secs` to `leave_secs`, "leave in 7 min";
                `Itinerary::new` is the one place they are derived.
                `notice::plan_notice` is
                the one place a compared plan's notice is computed:
                structured (no_trip / unreachable / later / diverted,
                minutes lost, routes, skipped stops) plus a text
                line, firing whenever the diverted world arrives
                later, has no trip, or rides a trip instance whose
                patched times differ from its unpatched one.
                Transfers are footpaths: straight lines (stop pairs
                within 300 m at 1.3 m/s) until `with_walks` replaces
                them with a walk table's walks — exactly the table's,
                between the network's stops (D-32). A plan's legs
                bound its rides, never its walks: the walk to the
                first boarding and from the last alighting settle
                before any round, so a one-leg plan walks to and
                from its bus. Its arrival and rides fixed, a plan
                then walks the least it can: the first ride boarded,
                each transfer made, and the last ride left at the
                stops whose walk is shortest and still connects
                (D-35). `walk_path`
                draws a walk leg: through its stops, and along a
                place's walk path where it walks from or to a place
                (a `PlaceWalk` carries the path `access` answered;
                between two places, the direct walk's).
transit-codec   The network codec, both directions, over the whole
                GTFS Schedule spec: `Feed` (src/feed.rs) reads and
                writes every spec file and column as the exact text
                the feed carried — lossless by construction,
                deterministic zip bytes — and `import` / `export`
                map a feed to an authored network document and back.
                A feed's trips ARE the document's trips: the
                agency's ids, every scheduled time exactly
                (offsets from the trip's first arrival), frequency
                trips as frequency trips, blocks by trip. A feed's
                shapes ARE the document's alignments: the feed's ids,
                every point and published distance, shapes no trip
                runs included. Patterns are stop sequences along an
                alignment — one per (route, direction, stops,
                exactness flags, shape, and what each stop row says:
                headsign, pickup / drop-off, continuous stopping,
                booking rules, published distance); how long a trip
                takes is its own. A trip with a row that is not a
                stop at a time (a location group, a zone, a window)
                is a demand-responsive trip. Everything else a feed
                carries reads into its model home and writes back
                value for value (src/extras.rs; text ⇄ values in
                src/values.rs). A row is exact (the pattern's
                timepoint) when it carries both times and does not
                say timepoint=0. The `Boundary` is the import's
                serialization residue (stop_sequence and
                shape_pt_sequence numbering, a frequency trip's
                stop_times base, the id given an agency published
                without one, which file names a route's fare
                network, and the services published as dates); an
                authored document exports with the default. Service
                ids are service types, written back as authored (the
                calendar row, exceptions as calendar_dates); the
                import keeps every calendar the feed names. A service
                published as DATES alone (no weekday flagged, every
                calendar_dates row an added date — C-TRAN's whole
                calendar) imports as the weekly rule its dates read
                as (D-105): a weekday qualifies when it runs on at
                least 4 of them and three quarters of that weekday's
                occurrences in the span (D-111); one range over the
                dates on those weekdays, a removed date for each one
                skipped, an added date for each other date — it runs
                on exactly the feed's dates. No weekday qualifying,
                it stays dated. The boundary keeps how it was written,
                so the export writes it back as dates (its zero-flag
                calendar row as written, every running date added),
                after a fork has carried it too
                (tests/dated_calendar_law_test). Each
                pattern gets one all-day timing profile from its
                earliest trip (the production tool), and each block
                layover is the observed gap between consecutive
                one-start trips of one service type (the planner
                re-authors from there). GTFS direction_id becomes
                the route's directions ("0"/"1"), named by the
                trips' most common headsign (else "To" the commonest
                terminal); the export writes direction_id back and
                each trip's own headsign (none invented). What
                the model cannot hold yet is reported as an
                ImportNote (stops without coordinates and the trips
                serving them, single-stop trips, a transfer point
                where fewer than two routes meet) — never silent.
                A transfer point is the feed's own flag: nothing infers
                one, so a feed that never sets it counts none (RAPTOR
                plans transfers regardless).
                The laws: the import is valid by construction over
                spec-valid feeds (tests/import_prop_test), keeps the
                trips (tests/trip_import_law_test), and
                `import ∘ export` is a fixpoint
                (tests/codec_prop_test); tests/spec_round_trip_test
                holds GTFS → network → GTFS against the whole spec,
                one law per file, oracle read as raw tables. The
                feed layer also answers a feed's summary and its
                feed_info identity, and base64 for the JSON seam.
                A LIBRARY under cap-network — the extraction
                pattern: crate boundary first, capability wrapper
                second. The C-TRAN import regressions live in its
                own test suite (tests/import_regression_test).
transit-analysis Planner analysis over an authored network document:
                stats, the service groups (service_groups: one row
                per service combination, its dates and its members'
                pooled numbers — which ARE the service on each of its
                dates, the laws pin it against the date-scoped reads;
                no date stands in for a group, D-55), the per-type
                overview (each type's authored service, on no date),
                a combination's detail and schedule texture (hourly
                profile, headway gaps, clockface),
                route corridors (shared stops and segments), the
                frequency grid (route_frequency: each route of a
                service combination hour by hour at its headway stop —
                the trips' own shortest, longest and median gaps,
                never regularized — with the route's numbers), a
                hub's connections by route pair (hub_connections:
                timed_transfers folded), and coverage of a WEIGHTED LAYER
                (its `coverage` module, D-47): any GeoJSON points and
                polygons, weighted by whichever numeric property the
                question names (`field`, with the planner's `label`) —
                the share a service COMBINATION serves and serves
                frequently (type_coverage: its members' pooled
                authored service, no date standing in — on every date
                it holds, that date's coverage; frequent counts
                departures only, D-6), the unserved where
                (unserved_points: the complement, named — a polygon by
                its unserved part), and the churn between two sides the
                host names (coverage_compare: gained and lost, D-99). A
                polygon's weight spreads over its area on a lattice
                (D-98) and is served by the share of it within the
                radius; every answer states the layer's resolution
                (point_spacing_m, sample_spacing_m).
                A LIBRARY under the
                cap-analysis module; the planner's regression
                numbers live in its own
                test suite (tests/regression_test).
transit-dispatch The materialized day's views, model-native: patch_day
                (skip/shift over MATERIALIZED stop times — the day,
                never the calendar) and cancel_day (a cancelled trip
                instance out of the day, every other trip whole),
                edit validation against the NETWORK (a cancel's trip
                must exist and run the instance it names), THE board (stop_board: walking-group
                pooling for a stop with nothing of its own,
                realtime-first over the live document — rows
                ordered by when the bus actually leaves (a visit the
                document cancels or skips is no row: D-73), each with
                its expected time, delay, status, and wait (due,
                minutes rounded down under an hour, then the clock
                time — `wait`, the one rule), its route's short and
                long names, its bay's name and platform code, and its
                destination as the sign says there; the departures
                the day's edits took away (`skipped`, each with the
                diversion that skips it or the cancel that took its
                trip — a dispatcher's cancel is listed, unlike the
                live document's, D-100 — the `Day` holds the
                unpatched day beside the patched one, one
                materialization: `day` and its key, which the kernel
                holds); the live
                document's alerts about the board, each row naming
                its own), the grouped board (grouped_board: THE
                board's rows by route and destination for a stop's
                display — every group the stop is served by all day,
                one that has stopped kept with none, its next
                per_group, its skipped visits; in route_sort_order,
                then id, then destination; `with_next_service` gives a
                stopped group its `next` — the first of the next seven
                days, D-42, whose patched day carries it, each day
                asked through a closure so the capability decides where
                the days come from, never the live document), the day's
                blocks list, the fleet summary (peak concurrent
                vehicles and when, platform / revenue / layover
                hours), the service keys
                running on a date, and the AUTHORED block chain with
                the layover ripple (layover_remaining_secs goes
                negative when a connection is eaten), and the
                DIVERSION EDITOR over the network (divert.rs: a
                draft's view — the route's patterns and the longest
                as default, each visit's role, the interior skipped
                unless served, the alignment, the flattened trace
                and its length, the problems — the record a ready
                draft becomes, the trip cancel a pick becomes
                (cancel_of: the trip, one instance or all, on a date
                its service runs), and the edit document's named rows
                — diversion rows and cancel rows),
                and WHERE SERVICE RUNS TODAY (geometry.rs: a trip's
                line over the effective day — the authored alignment,
                and for a diverted trip the alignment to the first
                kept stop, the traced detour, the alignment from the
                last (a skip-only diversion keeps the alignment) —
                and the ridden segment, that line sliced between two
                stops).
                A LIBRARY shared by the cap-board, cap-runcut,
                cap-calendar, and cap-divert modules; the UI reads
                them here. Holds no GTFS
                layer (the justfile arch gate enforces it).
transit-rt      prost types from the official GTFS-rt proto, the
                OVERVIEW view (delays, vehicles, each trip
                descriptor's start_time) over them, and the joins
                over the NETWORK: an rt trip id IS a model trip id,
                the instance its start (a frequency trip's, the
                descriptor's start_time — `resolve_instance`). The
                PROGRESS join (one trip's predicted times; the
                ranked late view with the DELAYED-BUT-DARK flag)
                pairs stop updates by the published stop_sequence
                (the import's `Boundary`; 1..n without one), by stop
                id only when an update names none. The BOUNDARY
                RESOLUTION (live_day): snapshots (trip updates,
                vehicle positions, service alerts) plus the network
                (and its boundary) → THE live document, the one
                place rt trip ids are resolved (tests/rt_join_law_test)
                and the one place an alert's informed entities become
                routes, stops, and trip instances — kept when in
                force at the snapshot's moment (tests/alert_test).
transit-live    THE live document: one rt moment in model vocabulary
                (delays per trip instance (trip, start) and stop,
                vehicles on their trip instances, every status classified under the
                on-time window the document records — one window,
                never a threshold per view; by default ANY earliness
                is early, because a bus leaving early is a missed
                bus — what does not run: a trip instance the feed
                cancels (CANCELED/DELETED: `canceled`) and a stop it
                skips (SKIPPED: `skipped`), asked through one rule,
                `LiveDay::runs`, by every reader — and the service
                alerts in force, each a set of
                scopes: route / stop / trip instance, fields ANDed,
                scopes ORed, all absent = the agency), the
                classification, runs and alert-matching rules, and the
                status summary. Pure data; no GTFS
                layer (arch gate). Minted by cap-rt, read by
                cap-board — no host computes a delay.
transit-version What every persisted document says about itself
                (D-115): `kind_tag!` declares a kind's tag (written
                always, an absent one read as the kind, another
                kind refused), `Version<N>` a version field whose
                absence reads as N, and `Versioned` what a reader
                needs to refuse another version naming both
                (`cap_abi::parse_versioned` at every kernel load,
                `cap_abi::Current` on a request field). Each owning
                library declares its kind's tag; the kinds and their
                versions are docs/FORMATS.md's "Versions".
transit-sync   THE sync document: EditRecord, FeedManifest, EditReport,
                the edit document (THE DISPATCHER'S OWN — it lives in
                the sync contract, not the GTFS codec) whose records
                carry exactly one payload: a Diversion or a
                TripCancel (a whole trip — one instance or all — out
                of service on one date, D-45, D-101; an absent
                payload is omitted, so a diversion record's bytes
                predate the cancel),
                pure validation/merge logic (merge is a
                property-tested semilattice; submit transitions
                drafts with the host-passed clock), and the record
                envelope every writer follows (propose and
                propose_cancel: rev 1 or bumped; set_retired: the
                tombstone and restore; read_document: a received
                document in the version this build reads, its
                records under the current schema, the unparseable and
                any record without exactly one payload dropped and
                counted). Shared
                by every host — cap-sync is the seam the Go server
                calls, cap-divert the authoring seam.
transit-plans  THE service-plan chain: resolve (which pick is in
                force), periods (each member's governing window),
                validate (chain invariants) over the plan set's
                metadata — DERIVED, never stored, deterministic and
                order-insensitive. And the PUBLISHED plan set (store
                kind `plans`, `PublishedPlans`): every published
                pick's metadata and network hash, canonical
                ((effective_from, id) order, one entry per id) —
                publish (the next set, or every reason the pick is
                refused: the chain, or a new, restored or moved pick
                dated before today, D-76), retire (a pick that has not
                taken effect, kept as the tombstone, D-75) and
                in_force (the published pick, and so the network,
                governing a date — retired picks never do). Cross-document logic, the
                counterpart of transit-sync's cross-record merge;
                cap-plans is the seam.
transit-runs   THE runs layer, beside the network (D-16). The
                WORK-RULE document (`WorkRules`, kernel kind
                `work_rules`; D-50): generic, every rule optional, an
                agency's values as data — recovery per route and/or
                service type (minutes and/or a whole percent of the
                run before the wait, the larger; the most specific
                rule answers), the route pairs one vehicle may
                interline (D-46; a route always follows itself, an
                unlisted pair never), the run rules (platform, spread,
                report and clear, pieces, a meal break, relief
                points) and pay (in time — no dollars, D-51);
                `checked` validates and normalizes it (the kernel's
                load runs it, so two orderings are one identity).
                The RECOVERY view (recovery): every wait between a
                block's adjacent running trips on a day, the recovery
                required there and the slack above it, folded per
                pattern into layover-to-runtime — short exactly where
                validation says layover_infeasible. SUGGESTED BLOCKING
                (suggest_blocks): the trips no block carries, by
                start, each following the vehicle that leaves it the
                least slack where it starts (the same stop, or a bay
                of the same station — no deadheads yet), on a pair the
                rules interline, after the recovery required; with the
                peak the adoption needs against the authored peak and
                the edits (add_block, add_block_trip, set_layover)
                that adopt it. The runs cut itself is open
                (`docs/units/runs-layer.md`). Pure; cap-runcut and
                cap-author are the seams.
transit-streets THE streets model. `model.rs`: the streets document
                (`Streets`: ways — points as [lat, lon] microdegrees,
                one-way, class, name), CANONICAL so its hash is the
                streets' own, not a file's: the same streets in
                another feature order, digitized the other way, drawn
                twice, or carrying other properties are one document
                (two-ways in canonical direction, a one-way running
                first point to last, ways sorted and unique;
                `validate` holds a stored document to it).
                `geojson.rs`: the boundary — a street layer (an
                agency GIS export such as C-TRAN's Trapeze roads,
                TIGER, OSM; GeoJSON lines) read into the document:
                CFCC, MTFCC, or OSM `highway` classify each street
                and leave out lines that are not streets (railroads,
                water — D-30), `FULLNAME` / `name` / `NAME` name it,
                TIGER `OneWay` and OSM `oneway` set its direction; a
                malformed vertex breaks a line. No other code reads a
                layer's properties. `lib.rs`: the routing graph built
                from the document — segments numbered way by way, the
                address a snapped position names (valid for one
                streets hash, so a trace commits GEOMETRY, never an
                address: a network never depends on a streets
                document) — snap (nearest street position) and
                shortest-path route (one-way aware; turn restrictions
                are a non-goal until a restriction table ships in the
                data), naming the streets the path follows in order
                (class, name, meters along each). `bus.rs`: the bus
                profile (D-26) — a bus is drawn along every class,
                never refused; unknown and the path-like classes
                (path, steps, track) are the streets a planner
                confirms. `walking.rs`: how long a rider walks — the
                walking profile is every class but motorway, ramp and
                unknown, both ways along one-way streets; a walk is
                each stop's walk to its nearest walkable street, the
                street distance, and the walk on to the other stop,
                at 1.3 m/s by default, kept within ten minutes by
                default, symmetric by construction. The WALK TABLE
                (`WalkTable`) is a document derived from a streets
                document and a network — every walk between its stops,
                naming both hashes, the speed and the limit — that
                the router plans transfers over. A POINT walks as a
                stop there would (`walks_from`): its walks to the
                stops within the limit, nearest first, each with the
                path the rider walks (the point, along walkable
                streets, the stop) — a router place's walks, the ends
                of a trip from anywhere. Walking quality is
                the streets document's: a source with real classes
                and footways (OSM), not one whose classes cannot tell
                a freeway from an arterial (QAF-85). Open work:
                `docs/units/streets-model.md`.
cap-authz       The authorization capability: may this identity
                perform this action on this pointer? One op
                (`authorize`), stateless like cap-plans — the host
                verifies the IdP's JWT and pushes the policy document
                per call; the verdict is fail-closed and never depends
                on rule order (transit-authz is the library, property-
                tested). The policy is a store document (kind `policy`,
                D-61).
cap-sync        The sync capability: the converged state of an edit
                document — its lifecycle facets: sync_edits (LWW merge
                + the submit transition, with the receive path
                clamping client clocks to the host's, D-56; the
                answer carries the canonical document bytes and their
                hash — the op behind POST /edits), read_edits
                (the receive: the current version only — an older one
                is cap-migrate's — typed parse, defaults fill what
                age removed, malformed dropped and counted), and
                compact_edits (the D-57 tombstone-retention commit).
                Stateless; validation is NOT here (what a record
                amounts to on a network is cap-divert's reports —
                one question, one answer).
cap-plans       The plan-chain capability: which service plan
                governs a date — resolve / periods / validate over
                the plan set's metadata, and publish / retire /
                in_force over the published plan set (publish and
                retire take the host's date) (transit-plans is the
                library). Pure ops, no registries: the host owns
                the plan set and pushes its metadata per call;
                publish answers the next set's canonical bytes and
                hash, which the server stores as given.
cap-author      The authoring capability: what does this network
                become after these edits? apply (typed edits,
                atomic) answers the next document under its hash,
                registered for the next change, with its issues
                and warnings (with work_rules_hash, a block's waits
                judged under the work rules' recovery — D-9).
                transit-network's edit module is the library;
                `created` names the entities a change made,
                `touched` the ones it altered beyond that (the
                patterns a redrawn alignment moves).
                Every host authors through it — the service-design
                UI included (its host sends edits and adopts the
                answer; it never edits a document).
cap-divert      The operator-edit authoring capability: what does
                this operator edit — a diversion or a trip cancel
                (D-102) — amount to over the network (draft — a
                diversion editor's whole view; list — the edit
                document's rows, names resolved: diversions in
                `rows`, trip cancels in `cancels`; reports — what each record
                amounts to on the network: valid, or what is wrong,
                and the trip instances it touches — invalid records
                REPORTED, never rejected, and a day view patches
                nothing for them), and what is the edit document
                after propose / cancel / retire / restore (the
                envelope rules; cancel refuses an unknown trip, an
                instance it never runs, a date it does not run).
                Every op takes the edit document whole (`edits`)
                and answers the next one tagged, so a host persists
                or posts the answer as given; another version is
                refused naming both (`cap_abi::Current`, the
                version check every kernel load shares). Reading a
                received document is cap-sync's read_edits. A separate need
                from cap-sync's converge step (the Go server hosts
                that one without a network). The host passes the
                clock and the record ids. No browser UI authors an
                edit yet: drawing a diversion is
                `docs/units/ui-disruptions.md`'s question 4, and
                cancelling a trip its question 6.

cap-migrate     The migration capability: what is this older
                document in the version this build reads? migrate
                brings a network (v1, every untagged shape → v2), an
                edit document (v1, the bare array → v2), or a
                watchlist (v1 → v2) forward through every step of its
                chain — the chains live in the owning libraries
                (transit-network, transit-sync, transit-dispatch),
                kept forever (D-66), every record id kept and every
                loss a note — and answers the new document's canonical
                bytes and hash; version answers whether a document
                needs it. Consumers read the current version only and
                refuse any other at the load (`cap_abi::speaks`), so
                no consumer carries an old chain. Hosted where an
                older document arrives: the Go server's receive and
                startup, service design's restore
                (docs/FORMATS.md, "Versions").

Every module is the raw rustc output of an assembly crate with the
same four flat exports (no wasm-bindgen — do not reintroduce host
glue).
```

## server/

Go + wazero host: scaffolding only (HTTP, manifest, proxies). It must
never make a domain decision; the host reads the feed's own
feed_info.txt version field (one boundary CSV field — I/O, not
domain). It instantiates the single-capability modules it needs
(it links no assemblies — D-78: nothing it holds long is a network,
and authz keeps its own instance), all rebuilt by
`just caps` into web/pkg/: cap-sync (the -wasm flag, cap_sync.wasm),
the op behind POST /edits, held for the server's lifetime; cap-authz
(-authz-wasm) when authentication is on; cap-plans (-plans-wasm) in
store mode, the op behind POST /plans; cap-network
(the -network-wasm flag, cap_network.wasm), which imports the
configured GTFS zip (the kernel's `load_feed` + `import`) — and canonicalizes and
validates each published pick — in a fresh instance
that is closed afterwards, so its working memory goes with
it; and cap-streets (the -streets-wasm flag, cap_streets.wasm),
which imports the configured street layer (`import`) and derives
the walk table over it and the published network (the kernel's
`load_network` + `walks`) in a fresh instance, closed the same way. Machine code
is cached per module bytes for the process, so a re-import does
not recompile. The manifest pins the network it publishes, the zip
it was imported from, the published plan set (store mode), the
streets and walk table it publishes, and the sync module's hash. (Reentrancy and the Go-host serialization live in
`docs/CONVENTIONS.md`.)

**The deployment publishes the network, not the feed.** Browsers
never parse GTFS a server has: the import runs once per published
zip, on the server, through the module the browser runs (same
bytes, same `network_hash`), and every client registers the
answer. On the real C-TRAN feed that is the difference between
~2.7 s of import and a ~187 MB network module on every rider's
page, and ~0.45 s and ~9–12 MB per registration of the network
(from its 680 KB gzip stream; wasm memory keeps a module's
high-water mark) — once per page, in its linked assembly. A zip loaded by hand
(the UIs' file input) still imports in the browser.

**The deployment publishes the streets the same way.** The street
layer is read once, on the server (`-streets`), and the walk table
— every walk between the network's stops, 13 s of work on C-TRAN —
is derived there too: a browser registers the canonical document
by its BYTES (the kernel's `load_streets` bulk op parses it in the
module; the page never parses it) and the table in the router. On
C-TRAN's Trapeze roads (`sha256:48fe6c89…`) the server's startup
spends 11.8 s on the import and the table; the page fetches 4.5 MB
(the document, gzipped) and 43 KB (the table), and registers the
document in 0.47 s; the first `access` builds the walking graph in
0.32 s, later ones answer in ~14 ms.

With `-store <url>` the server keeps a content-addressed store in
object storage (`docs/FORMATS.md`; D-113): `s3://bucket/prefix`
(S3 or an S3-compatible endpoint, `?endpoint=` naming it),
`azblob://container/prefix`, `gs://bucket/prefix`, or `mem://` (gone
with the process). The engine (`server/store/`) runs over one small
backend interface (`backend.go`: get, stat, create-if-absent, list
from a key, delete) and assumes only its contract — no append, rename,
fsync, or lock; the in-memory backend and the S3, Azure Blob and GCS
adapters (`s3.go`, `azure.go`, `gcs.go`) all pass one contract suite
(`contract_test.go`; `just store-contract` runs it and every engine
law against local SeaweedFS, Azurite and fake-gcs-server). Immutable
blobs are `blobs/<hex>` objects with the typed envelope as metadata,
and each commit is one record object `pointers/<name>/<seq>.json` —
the pointer is its highest record, and a commit is a blob create plus
a conditional create of record N+1, which is the CAS on the `from`
hash, then a re-read of its blob. Several server processes may share
one bucket; each caches heads and reads at most 2 s behind another's
commit. The `manifest` pointer is the root (publishing is one CAS on
it); the `edits` pointer is the multi-writer LWW document; the `plans`
pointer is the published plan set, and each pick's network is its own
`plan.<id>` pointer (D-74). The model (`store.tla`, `just spec`)
checks processes sharing a bucket with the offline collector beside
them.

A serving process opens the store with `store.OpenServing`: it
creates, reads and lists, and never deletes (D-117). The offline tools
are `server/cmd/store` (`just store …`): `gc` collects orphans with
its own delete-capable credentials, `verify` reads a whole store and
exits non-zero on the first break, and `copy` carries a directory
store an older build wrote into a bucket and verifies it.

The server's inputs come from their sources (D-118): `-feed`,
`-streets` and `-policy` each take an http(s) URL, `bucket:<key>` (the
object `inputs/<key>` in the store's bucket), or a dev path. A URL or
bucket source is re-checked at most once a minute (a conditional GET;
a stat), and only bytes that changed re-import. With `-store` each read
of the feed and the layer is committed as `source.feed` /
`source.streets`, the record naming its source; a source that does
not answer serves the last read, else that pointer's head — so a
process is disposable: a replacement given the bucket and the same
configuration serves the same manifest.

While the bucket does not answer (D-119) the server serves the heads
and blobs it holds (a byte-bounded blob cache; blobs are immutable),
labels each such read `X-Store-Stale` / `X-Store-As-Of`, and refuses
every write with 503 and `Retry-After`; the rider pages show the edit
document's age (kit's `staleLabel`). Without `-store`, `-edits <path>`
persists the edit document as one flat file and everything else is in
process.

Its HTTP surface:

- `/` — `web/dist` (stdlib `http.ServeContent`, so the Range
  requests PMTiles needs work). A file whose name carries its content
  hash (Vite's `assets/<name>-<hash>.<ext>`, the modules'
  `caps/<name>-<sha256>.wasm`) is served `immutable`; everything else
  (the pages, `caps/manifest.json`, `live-feeds.json`) `no-cache`, so
  an open page meets a new build on its next load.
- `/rt?url=<encoded>` — a proxy that fetches an agency's realtime
  feed and returns it with CORS headers, so feeds without
  `Access-Control-Allow-Origin` can be polled from the page (the
  server binds 127.0.0.1 by default; `-addr` changes it).
- `/geocode/{autocomplete,search,reverse}` — a proxy to the Pelias
  geocoder named by `-geocoder` (`<url>/v1/<op>`): it forwards the
  user's input only (`text`, the focus point, the reverse point) and
  adds the deployment's query policy (`-geocoder-policy`, default
  `server/geocoder.json`); 404 when none is configured. The geocoder
  is an oracle, never a document on the bus (D-36, D-38; web
  section, "External services"). `just geocoder-build` builds a local Pelias
  (`github.com/pelias/docker`'s portland-metro project, in the
  gitignored `.pelias/`, its transit importer not run — stops are
  the network's), `just geocoder` starts it on :4000, and
  `GEOCODER=http://127.0.0.1:4000 just serve …` (or `just dev`)
  proxies it.
- `/manifest` — `{network_hash, feed_hash, feed_version,
  plans_hash, streets_hash, walks_hash, wasm_hash, authz_hash,
  policy_hash, generated_at, publisher, timezone, services}` (the
  optional ones omitted when empty): which network is authoritative,
  the GTFS zip it was imported from, the published plan set, which
  sync module bytes the server runs, who says so, the agency's zone
  (the one the server counts its today in, and every page reads its
  clock in), and the external services it proxies
  (`services.geocoder: {path, query}` with `-geocoder`). Served
  `no-cache`.
- `/streets` — the canonical streets document imported from the
  configured layer (`-streets`), bare (the bytes `load_streets`
  takes); gzipped, its ETag the streets hash, re-imported when the
  file changes (mtime-checked). A client refuses it unless the
  module answers the ETag's hash (`servedStreets` in
  `src/engine.ts`).
- `/walks` — the walk table over the published streets and network
  (both `-streets` and `-feed`), derived again when either changes;
  gzipped, its ETag the walks hash, refused by a client the same
  way (`servedWalks`).
- `/network` — the import's answer for the configured zip
  (`-feed`), served as-is: `{network, boundary, notes,
  network_hash}`. Gzipped when the client takes it; its ETag is the
  network hash, so a client holding it revalidates for a 304.
  Republishing the zip is re-imported on the next request
  (mtime-checked). The planner's page takes it (it forks the
  network into a plan — `servedNetworkValue` in `src/engine.ts`).
- `/network/document` and `/network/boundary` — the same answer's
  two documents, split off for pages that register them without
  reading them: the bare network as gzipped bytes
  (`application/gzip`, never `Content-Encoding`, so the browser
  hands them to the modules compressed — kernel op 256 decodes the
  stream as it parses), and the boundary as JSON, both under the
  network hash's ETag. The document's bytes are the import's
  canonical serialization (unzipped, they hash to the pin). A
  client registers the document and refuses it unless the module
  answers the pinned hash (`servedNetwork`) — when no published pick
  is in force; a page loads the pick in force instead (web section).
- **Authentication/authorization** (D-58..D-61): with any `-oidc-*`
  flag set, the server verifies bearer JWTs — JWKS (`-oidc-jwks-url`,
  cached by `kid`, refetched on unknown kid) or a pinned PEM
  (`-oidc-key`), RS256/ES256 only, iss/aud exact, exp/nbf with leeway —
  and asks the hosted cap-authz module (`-authz-wasm`) over the policy
  document before any write: reads stay public (D-58), writes
  (`POST /edits`, `POST /pointers/*`, `POST /plans` — write on
  `plans`) need a role the policy grants write on that pointer. 401 bad/missing token, 403 denied; the
  verified `sub` is the store log's `writer`. The policy is the store's
  `policy` pointer, seeded from the `-policy` source (a URL,
  `bucket:<key>`, or a dev path) at first boot (D-61);
  later changes are authenticated CAS commits. No `-oidc-*` = auth off,
  logged loudly (dev only). `STORE`/`OIDC_*`/`POLICY` env vars pass the
  flags through `just serve`/`just dev`. What the server leaves to the
  deployment: the first JWKS fetch trusts the IdP's HTTPS (pin its
  certificate at the proxy for more), throttling repeated 401s is the
  reverse proxy's, and a browser gets and refreshes its IdP token on
  its own, presenting it on writes only.
- `/edits` — the edit document: `POST` a client document, receive
  the LWW-merged result (cap-sync's `sync_edits`, with the receive
  path clamping client timestamps to the server clock, D-56); `GET`
  reads the server's. With `-store` this is the `edit_records`
  pointer, CAS'd through the store (a CAS conflict retries against
  the re-read document); without it, `-edits <path>` persists it as
  one flat file across restarts, else in memory.
- Store mode also exposes the generic surface (`-store` only):
  `GET /pointers/{name}`, `GET /pointers/{name}/log?since=<hash>`
  (delta sync over the append-only log), `POST /pointers/{name}`
  (`{from, …}` → 200 + new head, 409 + current on conflict), and
  `GET /blobs/{hash}` (raw bytes, ETag the hash, envelope in headers;
  gzipped as `application/gzip` for `Accept: application/gzip` — except
  kind `policy` blobs, which are never served: 404). `GET
  /manifest` serves the `manifest` pointer's blob, re-committed when the
  published feed, streets, module, or services change. With auth on, the
  log endpoint requires a read (an `audit` rule implies it) and redacts
  `writer` unless audit is granted; `GET /permissions` (any valid token)
  returns the caller's effective-permissions projection (cap-authz
  `effective`) — the only client-readable form of the policy.
- `POST /plans` (store mode) — publish a pick (docs/ROADMAP.md 1,
  D-74): the body is the pick's network in any serialization. A fresh
  cap-network registers it and answers what refuses it (`publishable`:
  its issues, and a pick with no service in its period — 422 with
  `issues` and `refusals`) and its canonical bytes and plan frame;
  cap-plans `publish` answers the next published set or why the pick
  is refused (422: the chain, or a pick dated before today, D-76).
  `POST /plans/retire` `{"id"}` retires a pick that has not taken
  effect (D-75). "Today" is the agency's date: the host clock read in
  the network's zone, as cap-network answers it beside the hash (the
  feed's import, else the last published pick's `document`) — the
  host's own zone only until one is known; `-today` pins it on a test
  server. The server commits the network to
  `plan.<id>`, then the set to `plans`, and the manifest pins it as
  `plans_hash`. Clients read the set and each pick's network from
  `/blobs`.
- `GET /feed?date=` — the GTFS zip in force on the date (default the
  server's today): cap-network `export` of the published pick in
  force (cap-plans `in_force` answers which), kept by network hash;
  before the first pick takes effect, the configured `-feed` zip
  itself (D-74); 404 when neither runs. ETag the zip's hash.

`go test ./...` in `server/` proves the server runs the exact
module bytes the manifest pins and answers what the browser does.

## web/

Svelte 5 runes + Vite, TypeScript end to end, as a multi-page
build: `/` is the PICKER (`index.html` → `src/picker/`), one card
per generated UI (below). Every page lives at the web root because
asset fetches (`caps/manifest.json`, `basemap.pmtiles`) are
relative. Every page is a generated UI; there is no other shell.

### Parity

An agent (caps-run), the Go server, and the browser a human uses
run the SAME ops and get the SAME bytes. Any answer the browser
computes in TypeScript is an answer the other two hosts cannot
give — so `web/` holds host glue (I/O, fetch and pagination,
IndexedDB, the clock, settings, polling, the sync POST, the
bus worker) and RENDER components (props in, events out,
formatting), and nothing else. A join, fallback, threshold, record
builder, or geometry rule in TypeScript is a parity break, however
small: it lands as an op instead.

Two rulings that follow from it:

- **The live document.** cap-rt answers "what is live" in MODEL
  vocabulary: an rt snapshot plus the network resolve into a
  kernel document of delays per trip instance and stop and of the
  service alerts in force, addressed by hash; cap-board (and the
  router) consume it by hash. Which alert touches which departure
  is the board's answer, never the host's. rt trip ids resolve
  INSIDE the op, never in a host. Thresholds (the on-time
  window) are op parameters.
- **Geometry is a model answer.** "What path does this trip take
  today" (diversions, the authored alignment, an itinerary leg's
  ridden segment) answers in an op; `web/` only draws the GeoJSON
  it is handed.

The WEB GATES in `just test` hold it: no component imports the
bus, no host code reads raw rt `stop_updates`, nothing clones a
document to mutate it (every change is a NetworkEdit through
cap-author), and the engine is the one op caller — no file but
`src/engine.ts` imports the bus (`src/bus.ts`), no file but the bus
spawns the bus worker, and no file but the worker loads or calls a
module. Each
gate's allowlist only shrinks, and all are empty. Beside them, the
DETERMINISM gate holds the modules pure: no clock, randomness, or
environment in any crate's library code (hosts pass time in), so an
answer is a function of its inputs.

File ownership:

- `src/mapview.ts` only shapes GeoJSON: every line the map draws
  is an answer from the bus (cap-router `itinerary_paths`,
  cap-directory `focus`); it turns `[{lat, lon}]` into
  `[lon, lat]` and computes no geometry.
- Components are RENDER-ONLY: props in, events out. No component
  imports the bus or a UI's host layer (the WEB GATES and the
  GENERATED UIs gates in `just test`).
- `src/engine.ts` is THE ENGINE — the one caller of a module's ops.
  A host asks `ask(capability, op, args)`, the generated request
  types with every `*_hash` argument taking a `Doc` (a document
  version: its kind, its value or bytes, the document it is read
  over, the hash its server pins). A generated UI's host names its
  page first (`assemble("<id>")`): every capability its linked
  assembly links answers in that ONE module instance (the bus worker,
  below), and a capability it does not link answers alone. The engine
  registers a `Doc` in a module instance on first use through the
  kernel's load of its kind (`load` answers the load's whole answer —
  a feed's summary, a layer's headcount), so a document registers
  once per page however many capabilities read it; it asserts every
  instance answers the same identity, and caches answers under the
  journal's key (the module's pinned hash — the assembly's — and the
  request's bytes, which name the capability in a linked module).
  `produced` adopts a document an op answered (an import, an `apply`,
  the live document) as registered where it was made — for every
  capability linked there; a document the page declined to receive
  (the stop display's live document, `with_document: false`) carries
  the ask that mints it again (`remint`), and `together` says whether
  capabilities share an instance (so a host may decline it). `available` says
  whether a capability assembled, and `ask` throws `Unavailable`
  when it did not. `servedNetwork`, `servedStreets`, and
  `servedWalks` read the deployment's published documents and refuse
  one that does not answer its pinned hash — `servedNetwork` in the
  first of the page's named readers that assembles, so no module
  holds a copy only to check it. `servedNetwork` takes the page's local
  date: when the manifest pins a published plan set (`plans_hash`) it
  asks the plans module which pick is in force (`in_force`) and loads
  that pick's network by hash (`/blobs/<network_hash>`, gzipped) with
  the default boundary; before the first pick takes effect it loads
  the import (`/network/document` and `/network/boundary`). It throws
  Unavailable when picks are published and the plans module does not
  assemble — a page never guesses which schedule runs. Either way it
  holds the network as the gzipped bytes the server sends: the
  page never unzips, parses, or re-encodes it, and every module it
  registers in decodes the stream itself (a heading asks the
  directory's `about` for the network's name). `servedNetworkValue`
  is the planner's: the network as a value it forks. A module may drop what
  it holds (see "The bus worker"): asking for a dropped document is
  the module's unknown-hash error, and the engine registers the
  document again (or mints it again) and asks once more — a host
  never sees it. `keep` names what survives: every module instance
  drops every document not named (nor read over by one named); the
  rider pages call it when they adopt a network and when the edit
  document changes, and service-design after every change it adopts
  (the plan set's current networks and the layer — each `apply` mints
  a network, and undo registers a dropped version again). `window.__bus` is the e2e's inspection (the
  module instance a capability answers in, what it holds, its memory
  and resets, the ceiling, a replay of the last op asked of a
  capability past the answer cache).
- `src/bus.ts` is the page's side of the bus worker, imported only by
  the engine: the one Worker, spawned on first use, and its calls as
  promises — it carries messages and decides nothing.
- `src/documents.svelte.ts` is the device's document store (every
  version hash-chained by `produced_by`; `current` is a pointer;
  IndexedDB-persisted — ride-now's watchlist lives there).
- `src/plans.svelte.ts` owns the service plans (the authored
  network documents service-design keeps on the device: plain data,
  wholesale-replaced, IndexedDB-persisted). Stored plans restore
  through a module: `restorePlans(readStoredNetwork)` reads each
  through cap-author (an `apply` with no edits answers the canonical
  document), so a plan saved under older field names comes back in
  the current shape; a bare `restorePlans()` restores them as
  stored, and `readPlan(id, readStoredNetwork)` reads one on first
  use. The host never sniffs a shape.
- `src/basemap.ts` is the basemap choice every map view shares
  (local pmtiles, else OpenFreeMap) and the MapLibre worker wiring.
  The archive is probed by reading its header with the one `PMTiles`
  instance the protocol then serves every tile from (no separate
  probe request, the header fetched once per page); the local
  style's glyphs and sprites are `basemap-assets/` beside the tiles
  when `just basemap-assets` installed them (probed, never
  configured), else protomaps.github.io. `pickStyle(near)` returns
  the view the map opens on. Each map component owns its map
  (`window.__map` is the e2e hook).
- `src/viewport.ts` is where a map's camera starts and when it
  frames the whole network — pure presentation rules (`startView`:
  the rider's last fix where tiles cover it, else the view the
  network was last framed on, else the archive's view;
  `shouldFrameNetwork`: only when nothing better is known — no fix
  pending, none arrived, the camera not opened inside the network;
  `sameView`: a camera already on a view is not moved to it). The
  point is that no camera move precedes the one the rider ends on:
  each fetches tiles that are thrown away. The last fix and the last
  network frame are the rider app's own device data
  (`ride-now:last-fix`, `ride-now:network-view`, D-90), kept in
  `localStorage` by its host layer and passed to RideMap as `start`
  and `framed` (RideMap reports a frame it gave as `onframed`).

### The bus worker

`src/bus.worker.ts` is the ONLY wasm loader: every module a page
assembles lives in ONE module Worker, off the thread the page renders
on. A generated UI's page runs ONE module instance — its linked
assembly (the manifest's `assemblies`, D-77): every capability it
uses over one kernel registry, so a document registers once per page
and crosses between capabilities by hash without leaving the
instance. A capability the assembly does not link — or every one,
when the assembly does not load — runs alone in its own module
instance. The worker validates the manifest at the seam, fetches each
module demand-driven, verifies its bytes against the pinned hash,
compiles and instantiates it, and degrades per capability (a module
that fails to assemble is `Unavailable` for the capabilities it
would answer; the rest answer). Each `ensure` answers a capability's
ROUTE: the instance it answers in, that module's hash, its index
there, and whether the module links several. Document bytes cross to
it per registration (copied, never kept there: the page owns the
documents, and a reset re-registers from them).

Wasm linear memory never shrinks — eviction lets a module reuse what
it freed, but a peak stays allocated for the instance's life. So the
worker holds a MEMORY CEILING per instance (128 MiB, D-70: twice the
largest working set measured on C-TRAN — service-design holding five
plans' networks, 66 MB; a rider page holds one network, 16–18 MB):
before an op (never a registration), an
instance past it that has answered an op since it was made — and was
not reset in the last minute — is replaced by a fresh instance of the
same compiled module, dropping every linked capability's state
together. The reply says so; the engine voids that instance's
registrations (a generation per instance) and its recovery registers
what the op names again, each document once. Modules are pure, so the
fresh instance answers the same bytes; the guards keep a working set
above the ceiling from looping. The e2e pins both bounds
(`e2e/ui-stop-display.spec.mjs`, "a display left polling stays
bounded"): sixty polls of distinct rt snapshots plateau the rt
registry and the assembly's linear memory, and a forced ceiling
replaces the instance once per op with the same answer. It pins the
one instance too ("the display runs one linked module", and
ride-now's first spec): every capability asked answers in the UI's
assembly, holding the network once.

### External services

Parity holds the bus's answers; it does not say what a host may do
with something OFF the bus. External things are sorted by what
their answer BECOMES, not by how they are fetched (`/rt` and
`/geocode` are the same proxy and different kinds):

| kind | its answer becomes | examples | rule |
|---|---|---|---|
| source | a document on the bus, by hash | the GTFS zip, a street layer, the GTFS-rt feeds | a module registers its bytes; ops read it by hash |
| presentation | pixels — no op reads it | basemap tiles, glyphs, sprites | the map's; nothing else touches it |
| oracle | an input the user could have given by hand | the geocoder | below |

**The oracle test: could the user have done it themselves?** A
geocoded point is a tap on the map; a geocoded name is a label the
rider could have typed. An oracle only makes an input easier to
give, so every op stays deterministic in its inputs and cannot tell
where a point came from. A service whose answer no user could give
by hand — another agency's itineraries, say — is not an oracle: it
is a source (its answer becomes a document an op reads), or it is
not used.

The contract:

- **A host may** ask the oracle with the user's input only (the
  text typed, the point tapped, where the map looks), render the
  answer exactly as given, in the oracle's order, and turn the item
  the user picks into the input the user could have given: a point,
  and the label it was shown with.
- **A host may not** rank, filter, merge, or threshold the answers
  (that is policy), or join them with a document (that is an op).
  When the answers must meet model data — D-28's "rank stops and
  addresses together" — the oracle is PROMOTED: its candidates
  become an input document, like an rt snapshot, and an op joins
  them.
- **Ops may echo an oracle's label, never branch on it.** The
  router repeats a geocoded pin's name in the itinerary (D-37); no
  op decides anything by it.
- **The query policy is the deployment's** (which layers, how far,
  how many): the server's policy file (`server/geocoder.json`,
  `-geocoder-policy`), applied by its proxy, which forwards only the
  user's input from the page. Every host that asks through the path
  asks the same question.
- **Discovery is the manifest's.** `/manifest`'s `services` lists
  each service the deployment offers (`{geocoder: {path, query}}`);
  one it does not list is off, and the frames that need it say so —
  the host never probes.
- **Transport is the deployment's origin**, so a user's queries go
  no further than the service the deployment chose.
- **Durability:** nothing durable keeps a query or an oracle's id
  for a result (Pelias's `gid`) — only the user-equivalent input
  (the point and its label). The oracle's data drifts; a saved trip
  end must not drift with it.
- **Failure:** an oracle only makes input easier; down or slow,
  the user types a stop, taps the map, or shares a location, and
  nothing waits on it.
- **Parity:** caps-run asks an oracle the way a page does (`--ask`
  through the deployment's path) and chains its answer into the
  ops; the answers themselves drift with the oracle's data, so
  sessions record them as reference, never as fixtures.

### Adjacent systems

The systems that run the dispatcher's day — CAD/AVL, workforce
management, the agency's scheduling system — are out of scope (D-122)
and touch everything here. Unlike a source, a presentation, or an
oracle, an adjacent system both reads this repo's documents and writes
into its records. Each fact has one system of record: this repo owns
the documents (the network, picks, plans, policy) and the authored
detours and cancels; the adjacent system owns its own rows (a sick call, an
extra-board fill, a swap, a vehicle assignment). The seam:

**Out to an adjacent system**

1. **Documents, by hash.** Their shapes are the registry's schemas
   (`registry/schemas/`, indexed by `registry/documents.json`;
   `docs/FORMATS.md` for the at-rest envelope), and the manifest is the
   index of what is in force. An adjacent system keeps a document as a
   read-only cache keyed by its hash — never a second system of record,
   and never a table it queries for domain answers: those are ops.
2. **Standard formats wherever one exists:** the GTFS zip in force
   (`GET /feed`); GTFS + TODS for the operations plan (D-126); GTFS-rt
   for the authored cancels and detours — this repo is an authoring
   surface for what GTFS-rt carries (D-124; no op encodes it yet,
   `docs/units/rt-publish.md`). The registry's schemas cover only what
   the standards cannot say.
3. **References carry their version.** An external reference names
   the document with the entity: a pick id with a trip id, a trip
   instance (trip, start, service date) with its `network_hash`. A bare
   id can name different things in different picks (D-123).

**In from an adjacent system**

4. **Only as records, through the receive path** — `POST /edits` or a
   pointer commit (`docs/FORMATS.md`, "The HTTP surface"), under the
   system's own credentials, so the log's `writer` names it; the module
   validates and the per-record LWW merge applies. Nothing writes the
   bucket directly.
5. **Effects in this repo's vocabulary, never their causes.** A sick
   call with no extra-board fill arrives as a trip cancel; the sick call
   stays in the workforce system.
6. **A pointer back to the source, opaque.** A record has no field for
   the source system's id yet (D-125); once it does, this repo stores
   and serves it for audit joins and never branches on it — the rule
   an oracle's label follows.

### Generated UIs

A generated UI is a layout built for a set of questions, made from
THIN components over its own host layer. Each one is a directory, and the build, the picker,
and the gates discover it. Nothing central is edited to add one.

```
web/<id>.html                  the page (copy service-design.html; change the script src)
web/src/uis/<id>/ui.json       the descriptor the picker renders: id (= the directory),
                               title, summary, questions, ops ("<cap>.<op>" — the
                               capabilities they name are what its linked assembly
                               links; the kernel's loads are not ops), components,
                               session (the dogfood session: date, feed, open
                               findings), spec, generated
web/src/uis/<id>/main.ts       imports ../base.css (+ maplibre css if it maps), mounts
                               Shell, then awaits the host's boot (the layout paints first)
web/src/uis/<id>/Shell.svelte  PLACEMENT ONLY: reads the host's answers state, passes
                               props, routes events to host functions
web/src/uis/<id>/host.svelte.ts the ONLY file of the UI that asks the engine; owns
                               I/O, the selection, and one $state object of answers
web/e2e/ui-<id>.spec.mjs       the UI's e2e spec (fixture feed: e2e/fixtures/)
```

Shared by every generated UI:

- `src/uis/base.css`: color tokens (`--surface --text --muted
  --line --hover --accent --accent-soft --bad --warn --ok`) in light and
  dark, the page base, `.wb-scroll` (a table that scrolls in its own
  box with an opaque sticky header), and the class names the shared
  components render with (`.muted`, `.table-scroll`, `.chip`,
  `.picker-row`).
- `src/uis/kit.ts` (host layer only): the clock — `today()` and
  `nowSecs()` read the instant in the AGENCY's zone (the manifest's
  `timezone`, read once at boot by `readAgencyZone()` before the page
  picks its network; a loaded zip's by `setAgencyZone()`), never the
  device's, through `src/clock.ts`: the agency's date, and seconds from
  noon minus 12 h on it (the GTFS anchor — midnight except on a
  clock-change day; the fall-back hour before it clamps to 0).
  `whileVisible()` is every page's poll: a hidden page makes no
  requests and re-reads on its return. The answer graph is TRANSIENT — hand-built ahead of the flow
  maps `docs/units/host-discipline.md` designs, which replace it: a
  host declares its answers as `AnswerNode`s (the inputs each reads,
  the frame that renders it, when that frame is on screen), and
  `answerGraph` answers the nodes downstream of a changed input in
  declared order, yielding between nodes so each frame paints as
  its answer lands; a request made mid-pass supersedes the pass,
  and a hidden frame's node waits until it shows. It carries no
  domain knowledge. Every op a node runs is asked of the engine
  (`src/engine.ts`).
- `src/plans.svelte.ts` (`getAllPlans`, `putPlan`,
  `setActivePlan`, `restorePlans`, `readPlan`): the service plans.
  JS owns the documents. A host reads every plan it uses through
  cap-author before using it — at boot with
  `restorePlans(readStoredNetwork)`, or on first use with a bare
  `restorePlans()` and `readPlan(id, readStoredNetwork)` (the
  service-design host: a workspace of many forks reads the two it
  opens). The UI reads documents in JavaScript, which has no serde
  aliases; only the `plan` frame, which every stored shape spells
  alike, may be read before.
- `src/basemap.ts`: the basemap style choice (tiles, and glyphs and
  sprites local or remote) and the MapLibre worker wiring, for any
  map view.

The component catalog. Every component is THIN: props in, events
out, formatting only; it imports neither the bus nor a UI's
host layer (the gates check both).

- Thin, usable by any generated UI: Unavailable (a frame with no
  answer, said plainly: what is missing and why — an op or decision
  it waits on, or a capability that failed, with a retry), the
  service-design set: DesignRail (the views, keyed 1–6),
  ContextBar (plan, baseline, group, route, stop, hours, health, a
  route jump), PickTimeline (plans `periods` as bars, `resolve`'s
  plan named), PlanList (the plan set, fork, feed import),
  GroupTable (`service_groups`: each combination with its date
  spans), GroupLedger (`service_type_compare` totals), DayProfile (`hourly_trips`, a
  drag sets the hour window; `network_stats` on the same date: the
  service window, stops served, transfer points), FrequencyGrid (`route_frequency`:
  hours as cells, shaded continuously by the longest wait),
  CoveragePanel (the group's `type_coverage` with its parameters and window, the
  layer's weight field and label picked from the load's `fields`, its
  resolution from the answer's reading, the group's `unserved_points` listed),
  CoverageMap (`unserved_points` drawn: points as circles, polygons as
  their shape shaded by the share unserved),
  HubMatrix (`hub_connections`), CorridorTable (`route_overlap`),
  StopWaits (`stop_gaps`), RouteHeader (a route's `route_frequency`
  numbers, before → after), StringLine (directory `timetable` as
  time against stop; a dragged trip is a nudge event), RunTimes
  (`run_secs` by start), HourGaps (a route's `route_frequency`
  hours), GesturePanel (span, running time, one trip, stops — one
  event each), ChangeLedger (`service_type_compare` routes, undo and
  redo events), ProposalScore and Redline
  (`service_type_compare`), Readiness (the document's issues and
  warnings — network `validate`'s, or cap-author's for a version it
  just authored — the export
  and publish events — Publish posts the plan to `POST /plans` and
  shows the server's answer as given: published, or each refusal); RouteCard (`route_detail`), ScheduleChange (one
  pattern trips' starts with add and remove events), DirectionTabs
  (`route_detail` directions), TimetableGrid (directory `timetable`,
  added, retimed, and removed trips when asked against a baseline; a stop
  column click is an event), FocusMap (a
  directory `focus` drawn and framed),
  Combobox (a searchable picker), and the rider's set: RideMap (every
  stop, the live buses by status, a route's `focus`, a trip's
  `itinerary_paths`, the rider's location; it opens on the last fix,
  else the last network frame, and frames the network only by
  `viewport.ts`'s rules; a tapped
  stop or spot is an event), PlaceSearch (a debounced type-ahead over directory
  `stops`, and under them the geocoder's addresses when the host
  passes them — two lists, one box, D-28 — keyboard-driven), DepartureRows (a board's rows —
  `stop_board`, or a watched stop's from `watchlist_board` — with
  their expected time and status),
  StopCard (a stop at a glance), RiderBoard (one stop's board, its
  alerts, its cancelled trips, and the visits a detour skips), AlertList (the live document's
  alerts), TripPlan (the trip's ends, when, and the router's answer)
  with Itinerary (the answer as a timeline, door to door: leave → arrive,
  the trip's length, and for a trip leaving now "Leave in N min"), RouteList (directory
  `entities` for routes, filtered as typed), RouteView (one route's
  directions and their variants, as line diagrams), BusView (one
  live bus and its trip's `trip_progress`, stop by stop), and
  LiveFeeds (whether
  the times are live, and the feed URLs); and the stop display's set:
  DisplayHeader (the stop's name and id, the host's clock, live or
  scheduled), DisplayGroup (one `grouped_board` group as a station
  list's row, one route large, or a band across the screen; each
  time its `wait`, its status, and at a station its bay; a group with none left says it
  is detoured, cancelled, or when it runs next — the board's `next`,
  "Next: 5:12 am tomorrow", "Next: Monday 5:12 am" — or that nothing
  runs this week, and one with times left says when a trip of it was
  cancelled), DisplayAlerts (the alerts
  slot, always on screen), and DisplaySetup (the three layouts and
  the stop type-ahead). `rideFormat.ts` holds their formatting
  (clock times, a row's wait, status words and tones). Each
  frame whose capability can fail takes an `off` prop: a capability
  that did not assemble turns its frame off, and the frame says so.

The procedure. Every generated UI follows it in order, and a step
that is not done blocks the next. A UI's open work is its unit,
`docs/units/ui-<id>.md` (`docs/units/README.md`); the
unit is written before any code and deleted when the UI lands.

1. **Name the need and its questions.** Open the unit: the need the
   UI serves (named for the need, never a persona — a persona is
   one example of who has the need), and the questions it answers,
   in the order they are asked.
2. **Inventory the ops, from the manifest.** For each question:
   the op that answers it (`caps-run tools <cap>`), or the open
   finding that says none does (`docs/QA-FINDINGS.md`). This is
   the unit's first draft. It is a list of what to test, not yet
   a list of what works.
3. **Dogfood the questions on real data** through `caps-run`
   (`docs/TESTING.md`, the dogfood rules), one op per question.
   Each question that needs anything besides passing one op's
   answer through to the screen is an op gap, and it goes into
   `docs/QA-FINDINGS.md` as a finding. That covers a host join, a
   threshold, a derived number, a second op stitched to the first,
   bisecting inputs until an answer changes, or a rule the
   rendering would have to apply. A session re-confirms the
   findings it hits. The unit's inventory then names, per question,
   either its op (as a dogfooded fact) or its blocker (`QAF-N`,
   and `Blocked: D-N` when the fix waits on the user).
4. **Build only what is unblocked.** No host, shell, or component
   is written while the session in step 3 has not run. A blocked
   question is unblocked by an op (laws first, library first, the
   capability wrapper second — `crates/AGENTS.md`), never by host
   code. The UI may land answering only its unblocked questions:
   `ui.json` lists the questions it answers, and the unit keeps
   the blocked ones with their findings.
5. **Draw the answer graph before the layout.** In the unit, one
   row per node: the op (or a run of ops over the same inputs),
   the inputs it reads (documents, selection keys, the clock, a
   live snapshot, a visibility flag), the frame that renders it,
   and when that frame is on screen. The table is the UI's first
   draft of its flow map (`docs/units/host-discipline.md`): the
   engine will be designed from these tables, so a question the
   table cannot express (a timer, a stream, a node whose inputs
   depend on another node's answer in a way `needs` cannot say) is
   written into host-discipline.md as an engine requirement, and
   `kit.ts`'s transient runner is NOT extended to cover it.
6. **Write the host from the table.** `host.svelte.ts` declares the
   table's nodes as `AnswerNode`s and requests inputs from event
   handlers; every op a node runs is `ask`ed of the engine, over the
   `Doc`s the host holds. The only things a host may do: read and write I/O
   (files, fetch, IndexedDB), read the clock and pass it as an op
   argument, hold the selection (including repairing a selection
   the new answer no longer holds), track which frames are on
   screen, and adopt answers as-is. It never joins, filters,
   sorts, counts, thresholds, or falls back by a domain rule. A
   policy value is an op parameter whose default the op owns. The
   test: if `caps-run` or the Go server would have to re-implement
   it to get the same answer, it belongs in an op.
7. **Then the components, then the shell.** Thin components in
   `src/components/` (props in, events out, formatting only), then
   `Shell.svelte` (placement only).
8. **Land it.** Add `ui.json`, the page, and
   `e2e/ui-<id>.spec.mjs`; the host layer names its page
   (`assemble("<id>")`) and `just caps` generates its linked
   assembly from `ui.json`'s `ops`. Run the UI in the browser on the real
   feed (`just serve testdata/ctran-gtfs.zip`) and check its numbers
   against the runner session's. Record the session in the
   descriptor's `session` field, and describe the UI below. Delete
   the unit if no question is left blocked, naming its pins;
   otherwise edit it down to the blocked questions.

Gates (`just test`, GENERATED UIs and the WEB GATES): every `src/uis/<id>/`
has its `ui.json` (with fields complete, and id equal to the
directory), its page, and its spec. No `.svelte` file imports the
bus; no UI imports another UI's files; no component imports a
UI's host layer. The build also refuses a UI directory without
its page (`vite.config.js`), and the picker's spec
(`e2e/picker.spec.mjs`) checks one card per `ui.json`.

The UIs today: `service-design` (`src/uis/service-design/`): six
views over one selection — a rail of views (keys 1–6) and a context
bar (plan, baseline, service group, route, stop, hour window) frame
whichever is open. Plans: the pick timeline, the plan set (fork,
baseline, feed import), the service groups (the combinations of
service types that run together, each with its dates; the
day-scoped frames ask on a group's first date — any of its dates
answers alike). Network: the group's
numbers against the baseline, trips per hour (a drag sets the hour
window every view reads), and the frequency grid; a row opens the
route. Coverage: the parameters and window, the selected group's
served and frequent share (named by its members, with its dates), and
the places its stops leave unserved, listed and mapped. Hubs: a stop's connection
matrix and waits, and the shared corridors. Route: the string-line
workbench — directions, the string-line or the timetable grid,
gaps by hour, running time by start, the change ledger with undo
and redo, the route's map, and (edit mode, E) the gestures: span,
running time, one trip moved alone (picked or dragged), stops —
each acting on one service type of the group, which the panel picks
and names, warning what else runs with it and the other groups it
runs in (D-8, D-95). A frequency target is shown as waiting on D-54. Proposal: the
scorecard, the redline, readiness, the export, and publishing the
pick to the deployment (D-74). The selection
lives in the URL hash. With no stored plan it opens the server's
`/network` as the base plan. Its host answers through the answer
graph; a view's nodes answer only while it is open, each frame
records its own failure (`sd.failed`) and renders Unavailable with
a retry while the rest answers, and the plan's health adopts
cap-author's issues and warnings for a document it authored. After
every change it adopts, the host names its keep-set (the plan set's
current networks and the layer). Undo restores an
earlier document version the host keeps; nothing is inverted.
`ride-now` (`src/uis/ride-now/`): a full-bleed map with a panel over
it (a card at the left on a wide screen, a sheet over the bottom of
the map on a phone), the live pill and the alerts button in the
corner. The map draws every stop and every live bus (colored by its
status, pointing its bearing, labeled with its route); a tapped bus,
stop, or spot opens it. The panel follows the rider: home (the
type-ahead — stops, and addresses and places when the server runs
a geocoder: a picked address is a spot, "Near 1400 NE 136th Ave" —,
"Get directions" and "Routes", the places near the rider
or a tapped spot with each one's next departures, saved stops,
recent stops — nearest by the walk along the streets when the
server publishes them), one stop (its live board; a picked departure draws
its route, marks its bus, and offers to track it), one bus (its
trip stop by stop, the stop it heads to marked, followed on the
map), the routes and one route (its directions' variants as line
diagrams; its buses stay bright on the map, the rest dim), getting
there (the ends as type-aheads, or the rider's location, or chosen
on the map — "Choose on map" per end: the next tap sets it, a stop,
or with the streets published any spot, a dropped pin named by the
geocoder's nearest address (D-37); an address typed in an end's
field is a point end the same way; "Directions
here" on a tapped spot does the same; a point end plans
as a router place over cap-streets `access`, its walks drawn along
the streets; leave now, leave at, arrive by, or the last bus; the
itinerary as a timeline, drawn on the map), and the alerts in
force. It boots from the server's published network
(`servedNetwork`, bytes it never parses; or a zip the rider loads),
then loads the server's `/streets` and `/walks` in the
background (`servedStreets`, `servedWalks`; until they land, or
where none is published, walks are straight lines and trips run
stop to stop; a hand-loaded zip walks no streets — the walk table
walks the published network), reads the edit document from
`GET /edits`, reads the live feeds the deployment names in
`public/live-feeds.json` (the rider may set their own) through
`/rt`, and polls both and the clock every 30 s. A trip asked for
"now" is re-asked on each poll. Addresses come from the geocoder
the manifest lists (read once on boot; "External services"): the
host sends the typed text and the near point or the rider's, shows
Pelias's answer as given, and a point it answers keeps the name it
was shown with; nothing keeps the query. Its questions and session
are its `ui.json`.

`stop-display` (`src/uis/stop-display/`): the screen at a stop, a
passenger information display, in three full-screen layouts over
one `grouped_board` answer — a transit center (the groups as a list,
each time naming its bay, the alerts beside it), a single route (one
group filling the screen, its type scaled to it, for the stop one
route serves), and several routes (a
street stop's two or three, each a full-width band sharing the
screen's height, its type scaled to the band); the alerts slot is
always on screen. Any layout shows any stop. The stop and the layout live in the page's URL
(`?stop=<id>&layout=center|route|routes`), so a screen reopens on its
stop; without a stop, the setup picks both. It boots from the
server's published network (`servedNetwork`, bytes it never parses;
or a zip), reads `GET /edits` and the deployment's
live feeds (`public/live-feeds.json`, through `/rt`), and polls them
and the clock every 20 s. Its questions and session are its
`ui.json`.

Layouts are hand-placed Svelte; layouts as documents are
`docs/units/wire-time-assembly.md`.

Frontend conventions and gotchas: `docs/CONVENTIONS.md`.

## The capability bus

The architecture: horizontal capabilities, one NEED per capability,
assembled per user from a manifest — capability bytes differ per
user while the DOCUMENT and the contracts stay identical everywhere.
A MODULE is an assembly over the kernel (`cap-abi`): one capability
alone (`module-<cap>` — what caps-run, agents, the registry's blobs,
and the Go server run), or a generated UI's capabilities LINKED into
one module (`module-ui-<id>`, D-77 — what that UI's page loads), so a
document registers once per page and the shared code ships once.
Linking assembles bytes and merges no surface: each capability keeps
its crate, its Request enum, and its manifest entry. Linking a
user's own selection at wire time is
`docs/units/wire-time-assembly.md`'s. The assembly crates live in
`assemblies/`, generated by `gen-assemblies` from the capability
crates and each `web/src/uis/<id>/ui.json`'s `ops` (`just caps`
writes them; `just test` checks they are current).

```
cap-abi         The kernel and the seam: the flat C ABI as a macro
                (`assembly!` — every module is the same four exports,
                emitted once per assembly crate; a capability crate
                only REGISTERS with `capability!`, and forbids
                `unsafe`), the
                capability-manifest contract (speaks/migrates/
                consumes + agent context), the ASSEMBLY VALIDATOR
                (compose-time proof: closure, version coherence,
                migration-path reachability — total and property-
                tested; gen-manifest self-validizes before writing),
                and gen-manifest (the capability set as code; the
                manifest diff IS the reviewable assembly change; its
                worked examples are computed, and cap-author apply's
                carry one example per edit kind, applied to the
                fixture in order — a kind without one, or one that
                stops applying, fails the build).
                THE KERNEL (kernel.rs) owns the registries: one per
                document kind — network, edits, live, boundary,
                walks, streets, watchlist, layer, feed, rt,
                work_rules — and one
                derived registry, each a `Registry` (registry.rs)
                whose constructor is private to cap-abi, so a
                capability cannot declare shared state. Each kind's
                LOAD is a kernel op: a bulk op (256 network, 259
                edits, 260 live, 261 boundary, 262 walks, 263
                streets, 264 watchlist, 265 layer, 266 feed, 267 rt,
                269 work_rules) over its bytes, and the same JSON op
                (`{"op": "load_network", "network": …}`) peeled
                before any capability decodes the request (the
                document left raw, never buffered into a generic
                tree). A load answers the document's identity — the
                canonical hash of the parsed document
                (canonical_hash, streamed into the digest), or for a
                feed, a layer and an rt payload the content hash of
                its bytes — and what the host shows (a feed's
                summary, a layer's headcount, a streets document's
                size, a watchlist's version). A capability declares
                the kinds it reads (`capability! { documents: [...] }`)
                and reads them through the kind (`NETWORK.require`);
                what an op mints registers where it is minted
                (`NETWORK.register` — an import's network and
                boundary, an apply's next network, cap-rt's live
                document, a decoded snapshot). DERIVED entries are a
                library's pure function and its key, held under a
                `Derivation` and bounded per derivation (`derive`):
                the day (`kernel::day` — transit-dispatch's `day`,
                the effective trips and what the edits took away, one
                materialization), the router's graph, the street and
                walking graphs; a keep-set that drops any input drops
                the entry. Every assembly answers the registry ops
                (257 evict, 258 held) over the kinds its capabilities
                read plus the derived entries, and 268 about (the
                capabilities it links, each with its kinds). An
                assembly's JSON request names its capability
                (`"cap"`) when it links several; a capability's own
                bulk op (0–255) carries its index in the high 16 bits.
                The manifest's `kernel_ops` documents every kernel op
                with an example computed through the same kernel; each
                capability's entry names its `documents`, and
                `assemblies` records each UI's linked module (its
                capabilities in index order, its pinned bytes). Every
                capability's JSON lane answers through `answer` /
                `outcome`, serialized straight from the typed response
                — no generic tree, so an answer carrying a document
                (an import's network) carries its canonical bytes
                (struct field order, docs/FORMATS.md).
                gen-assemblies writes the assembly crates;
                arch-gates checks the dependency graph and every
                capability crate's source (below, "Bus rules").
                Also caps-smoke.mjs: Node ABI smoke + the law that
                every advertised op is documented (undocumented ops
                fail the build — agent context cannot drift), every
                module's `about` matches its manifest entry, and each
                linked assembly registers a document once for every
                capability; independence.mjs: the assembly-
                independence law (below).
cap-gtfs        The feed-document capability: the feed's OWN question
                over a GTFS zip the kernel registered (load_feed
                answers what it contains at a glance — its summary):
                its identity (feed_info).
                The day views, the watchlist, the pickers, and the
                names join are NOT here: they answer over the NETWORK
                document (cap-board / cap-runcut / cap-calendar /
                cap-directory — one question, one answer on the bus).
                Parse + identity alone, over transit-codec's whole-
                spec `Feed`. CHARTER
                CLOSED: parse, export, primitive boundary queries —
                nothing more goes "in GTFS."
cap-router      The router capability — ONE QUESTION: how do I get
                from here to there? — over the NETWORK document and
                the effective day — the model's materialized trips,
                patched with the edits in force (the same
                transit-dispatch composition every day view starts
                from), read at the live document's predictions when
                live_hash names one (cap-rt's live answer), its
                transfers walking the streets when walks_hash names a
                walk table (cap-streets' `walks` answer, refused when
                derived for another network; straight lines without
                one), the RAPTOR graph a derived entry per (network,
                edits, live, date, walks; only the newest live
                document's graphs stay). A plan's `from` and `to`
                are stop ids or places (`{id, name, walks}` —
                cap-streets' `access` walks pass as they are) with
                `direct` the walk between them (`access`'s). plan
                leaves at a time; plan_arrive_by answers the latest
                departure that arrives by a deadline (none: the last
                bus home). itinerary_paths, given the plan's ends
                and `direct`, draws a place's walks along their
                street paths (`transit_router::walk_path`); without
                them a place draws nothing.
                The compare facet is the diversion delta:
                the unpatched day read at the same predictions, one
                materialization apart, with its notice; the
                itinerary_paths facet draws a plan's legs over the
                same day (a transit leg's ridden segment, a walk
                through its via stops). The browser plans through
                plan_compare alone (no edits hash: no baseline) and
                shows its notice's text. A pure-function capability; it
                writes nothing.
cap-analysis    The analysis capability: planner numbers (stats,
                service types, the frequency grid, hub connections,
                coverage) over the kernel's networks and weighted
                layers (load_layer answers the features and the numeric
                fields a layer can be read by; a coverage op names the
                field and label, D-97). The domain code lives
                in the shared transit-analysis library. Consumes
                network documents from cap-network (the registry
                edge the manifest carries).
cap-network     The network codec capability: import (the planner's
                bootstrap over a feed the kernel registered — the
                document and its boundary residue register
                themselves, both hashes in the response, with the
                network's timezone: its operators' one zone, the
                one a host counts the agency's date in), validate
                (the authoring feedback loop: issues that gate
                export, warnings that never gate), export (the
                publish path — deterministic GTFS zip bytes under
                their content hash, base64 across the JSON seam,
                numbered by the import's boundary when boundary_hash
                names it; the zip registers as a feed), publishable (what refuses it as a
                published pick: its issues, and a pick with no
                service in its period, D-76), document (a registered
                network's canonical bytes, hash, plan frame and timezone — what the
                server stores when it publishes a pick it was sent
                in any serialization). The domain code lives in the
                shared transit-codec library.
cap-rt          The realtime capability: decode — a GTFS-rt protobuf
                payload (bulk-bytes op 0, the ephemeral bytes an
                agency serves every few seconds) into the overview
                view (delays per trip/stop, live vehicles, service
                alerts in the feed's ids), each
                snapshot registered under its content hash (the
                kernel's load_rt registers one answering the hash
                alone); and the SCHEDULE JOIN the dispatcher actually
                asks for, over the NETWORK and, for an imported
                network, its boundary (the published stop_sequence
                numbering; optional): trip_progress (one trip's scheduled/delay/predicted
                board; a stop the update skips carries the last
                stated delay, GTFS-rt's propagation rule), late_trips (ranked, with the DELAYED-BUT-
                DARK flag and vp_hash for agencies that serve the
                two feeds separately — C-TRAN does). Dark is judged
                per BLOCK: a delay propagated to a block's next trip
                while its bus runs the trip before is a located
                vehicle (position_trip_id names the trip it is on;
                vehicle_id rides beside the label; a vehicle with no label is named by
                its id). live mints THE
                LIVE DOCUMENT (transit-live) from the
                trip-updates, vehicle-positions, and/or service-alerts
                snapshots (alerts_hash; every snapshot's own alerts
                count, one per entity id), with
                the summary a status panel reads (vehicles by
                status, the late list), registered where it is minted
                — linked beside rt, the board and the router read it
                by live_hash and it never crosses to the host and
                back (with_document: false declines it in the answer
                too — the stop display, QAF-49); the browser's live
                views render it; none joins rt.
cap-board       The board capability — ONE QUESTION: when does
                service leave this stop? (at a stop, a station's
                pooled bays, or a personal saved set — the WATCHLIST
                document, v1/v2, read in either (cap-migrate
                migrates it): a personal
                multi-stop board is the same question parameterized
                by a personal document), over the kernel's network,
                edit document, live document (cap-rt's, by hash) and
                watchlist: stop_board (THE board:
                pooling, realtime-first order, row status, the
                route's long name, the bay's name, the sign's
                destination, the visits the in-force edits skip and
                the diversion that skips each, the live alerts about
                the board — the module's answer, never the host's) /
                grouped_board (the same rows by route and
                destination, the next three of each by default — the
                stop display's answer; a group that has stopped says
                when it runs next within a week, from later days it
                materializes itself rather than holding them, since the
                kernel's few held days would be pushed out by a week of
                them) / watchlist_board (each watched
                stop's stop_board, the same edits and live document;
                a trip's FINAL
                visit is an arrival, never a row) over ONE
                MATERIALIZED DAY (materialize(net, date), patched
                with the in-force edits — the kernel's held day, one
                materialization per (network, edits, date) however
                many boards ask, no GTFS expansion). The domain code lives
                in the shared transit-dispatch library; no GTFS
                layer (arch gate). The UI's departure board renders
                stop_board's answer; rows carry the trip instance's
                address (trip, start), so the click-through needs no
                join.
cap-runcut      The runcut capability — ONE QUESTION: what does
                this vehicle do today? The day's blocks LIST (the
                index facet: vehicles out, with that day's running
                trip counts; a block whose every trip is
                unrepresentable or skipped is not offered — the
                honest hole), the day's FLEET SUMMARY (peak
                concurrent vehicles and when, platform / revenue /
                layover hours, and each running block's outline —
                span, trips, longest internal gap — the fold that
                took a block_trips call per block), and one block's
                chain (the detail
                facet: the AUTHORED runcut with the layover ripple —
                the in-force edits' added time, or with a live
                document a trip's predicted delay at its
                last stop, which wins (D-18) —
                layover_remaining_secs going negative when a
                connection is eaten, and layover_health classifying
                it — overlap / tight / ok under the caller's
                tight_layover_secs; each row names its trip by id, a
                frequency trip once per instance) and one trip
                instance's EFFECTIVE
                stops on the date (the trip detail facet) and its
                LINE (trip_path — what the map's click-through draws:
                the authored alignment, with an in-force diversion's
                traced detour spliced between its kept stops).
                Blocking is where cutting the work begins (D-46), so
                the same question answers, under the WORK RULES (the
                kernel's `work_rules` kind), what each wait requires
                (recovery: required vs slack, per wait and per
                pattern) and which vehicles the trips no block carries
                need (suggest_blocks: suggested blocks, each wait's
                slack, the adopted peak against the authored one, and
                the edits author `apply` adopts). Same
                documents and held day as cap-board; the
                domain code lives in the shared transit-dispatch and
                transit-runs libraries; no GTFS layer (arch gate). The UI's blocks
                panel and trip lines answer through this module.
cap-calendar    The calendar capability — TWO READS of one relation:
                which service keys run today (active_services), and
                the reverse, the dates one service type runs
                (service_dates over an inclusive window). Both over
                the EFFECTIVE day (a key whose every trip is skipped
                or unrepresentable is not running, however its
                calendar reads — the same honesty as the runcut
                view's empty hole). Same documents and held day as
                cap-board; the domain code lives in the
                shared transit-dispatch library; no GTFS layer (arch
                gate).
cap-directory   The directory capability — ONE QUESTION: what
                exists in the network? What the network is (about:
                its name and plan — the heading of a page that holds
                the network only as bytes), the stop list (name-sorted,
                with the walking-group structure the station board
                keys on; an optional query narrows it the way a
                picker's typing does, fuzzy — exact matches first,
                then a sign's abbreviations, then typos, D-48), the
                names join (stops_by_ids),
                the PLACES nearest a point (stops_near: great-circle
                meters, nearest first, ties by id, at most `limit`,
                within `radius_m` when given; a station's bays answer
                once as the station, at its nearest member's
                distance — the walking group the board pools; given
                the point's `walks` (cap-streets `access`), the
                places a walk reaches first, by walk — the shortest
                to any member — and the rest by distance, D-34; names
                are matched by the sign's own words, never by an
                alias), the route list
                (the model's closed mode set; every row that names a
                route carries its long name — routes, entities,
                stop_detail's routes, the timetable header), and one route's
                PATTERNS with their ordered stops — what the
                diversion editor anchors against (new edit records
                carry direction_id null — the documented
                both-directions case). The pickers are
                model questions, never feed reads; the UI's
                comboboxes and the map's name lookups read this
                module (the feed line reads the kernel's load_feed
                summary + cap-gtfs's feed_info). focus answers where an entity
                is on the map — the planner's drill-down frames it.
                entities, route_detail, and stop_detail answer the
                planner's lists (filtered rows per kind — the forms'
                pickers and the Document counts too; a route's
                composition with trip counts and its patterns and
                rider variants by direction; a stop's serving
                routes — its own, or a station pin's bays', the
                board's rule), and timetable
                answers the planner's timetable (route + direction
                + service type, across the direction's patterns) —
                the planner computes no number.
cap-streets    The street-graph capability: where along the
                streets. import (bulk-bytes op 0) reads a street
                layer into the streets document and registers it,
                answering the document (the host keeps it) with its
                hash and size (the kernel's load_streets registers a
                document the host holds under the same hash, refused
                unless canonical); snap answers the
                nearest street position (the graph address a trace's
                waypoint commits to); route answers the street
                geometry between two positions (shortest path,
                starting after `from` — trace legs chain — with
                its length along the graph and the streets it
                follows; empty on disconnected parts — the caller
                falls back to a straight line; the bus profile's
                streets to confirm come back named, `confirm` the
                op's parameter);
                walks answers the walk table between a registered
                network's stops with its hash (the kernel's
                load_walks answers the same); access
                answers a point's walks to those stops, each with
                its path, nearest first, and the direct walk to a
                second point — the router's place. The walking
                graph is a derived entry built on first walks, the
                street graph on first snap — a host that only walks
                never builds the other. The domain code lives in the shared
                transit-streets library; the module writes nothing. Its consumers
                today are the server (the streets and walk table it
                publishes) and ride-now (walks from a point to the
                stops near it); drawing a detour or a pattern on
                streets is next (`docs/units/streets-model.md`,
                `docs/units/ui-disruptions.md`).
```

### Bus rules (load-bearing)

- Capabilities compose through DOCUMENTS addressed by content hash,
  never capability-to-capability — linked in one module, they share
  the kernel's registries; in separate modules the host loads the
  same bytes into each and ASSERTS the returned hashes agree. In the
  browser this holds by construction: a document is one engine `Doc`,
  and every instance it is registered in must answer the identity
  the first one did (and a served document the hash its server pins).
- CAPABILITIES NEVER CALL CAPABILITIES — in one binary a crate can
  still name only the code of crates it depends on, so the rule is
  enforced by four gates in `just test`, each broken on a fixture in
  its own test: the dependency graph (`arch-gates` over `cargo
  metadata`, deny by default: a capability depends only on cap-abi
  and libraries, only the generated assemblies depend on
  capabilities, only the codec's homes read transit-codec —
  `crates/cap-abi/tests/arch_gates_test.rs`); the kernel's private
  registry constructor (the `compile_fail` doctests on
  `cap_abi::registry::Registry`) with no state declared in a
  capability crate (no static, no thread_local); `#![forbid(unsafe_code)]`
  in every capability crate (the unsafe exports are `assembly!`'s);
  and ASSEMBLY INDEPENDENCE (`crates/cap-abi/independence.mjs`, its
  broken fixture `independence.test.mjs`): every worked example
  answers identically from the capability alone and inside each UI's
  assembly, in either order of its capabilities — a shared derived
  entry is only a cache, never a dependency.
- The MODULE is the hash authority — hosts never mint document
  identity (JS JSON.stringify differs from the module's canonical
  serde bytes in float formatting; query with the hash the module
  returned).
- Bulk op numbers: a capability's own number from 0 (streets'
  import, rt's decode) — in a module linking several, its index in
  the high 16 bits; kernel ops number from 256 and mean the same thing
  in every module that answers them: each kind's load (256 network,
  259 edits, 260 live, 261 boundary, 262 walks, 263 streets, 264
  watchlist, 265 layer, 266 feed, 267 rt, 269 work_rules) in every module whose
  capabilities read the kind, the browser host registering every
  document that way (the engine's loader per kind, the document
  encoded once however many instances register it). EVERY module
  answers the registry ops: 257 (evict — `{"keep": [hash, …]}` drops
  every document not kept, and every derived entry whose inputs are
  not all kept; it answers what dropped and what is held), 258 (held
  — what the module holds, per registry, oldest first), and 268
  (about — the capabilities it links, each with its kinds).
- The kernel keeps every document, in `cap_abi::registry::Registry`
  statics only it constructs: the module owns what it holds (it is
  the hash authority), the host names what survives (it owns the
  documents). Ephemeral kinds bound themselves — rt snapshots keep
  their last 8, live documents their last 4 — and so does each
  derivation (held days 4, router graphs 8 and only the newest live
  document's, street and walking graphs 4), so a poll loop plateaus
  with no host steering; durable kinds (networks, feeds, edits,
  boundaries, layers, walk tables, streets, watchlists) stay until a
  keep-set drops them.
  An evicted document asked for is the unknown-hash error every
  module speaks — `unknown <kind> <hash>: <load op> first` — never a
  wrong answer.
- One Request enum per capability is the whole surface, and no
  load is in it — registration is the kernel's (networks are minted
  only by cap-author's apply and cap-network's import — readers have
  no authoring ops by construction).
- A capability crate is an rlib that registers (`capability!`
  declares `CAPABILITY` and its native seam — `invoke`,
  `invoke_bytes`, `registries`, the capability alone exactly as its
  one-capability module answers, for tests); only an assembly crate
  emits the `#[no_mangle]` exports (`assembly!`), so linking never
  collides.
- Do not let a capability grow a private document format — shared
  data crosses modules as kernel documents addressed by content hash
  only.

### The registry and the runner

The registry (`registry/`, published by every `just caps`; `just
registry` runs the gates first): immutable blobs + one evidence
record per capability (contracts, agent context, which gates ran —
none for a plain `just caps` — at which commit, and the digest of
the source tree that built it) + a pointer. `caps-run` recomputes
the source digest on every run and says on stderr when the registry
was published from another tree (`just test` gates the agreement).
It is the
artifact agents and humans pull SANDBOXED COMPUTE from —
`tools/caps-run.mjs` verifies blob bytes against the pinned hash and
runs ops behind a worker-termination wall-clock guard (`--timeout`).
A session runs its steps in command-line order and may span
capabilities (`--cap` switches; each module verified and
instantiated once): documents cross between modules BY REFERENCE
(`"network":"$net.network"`), the way a host hands the same bytes to
each module, and `--as` names results so references survive
reordering. A session holds each result as the module's JSON text
and parses one only when a reference reaches into it — a parsed
real-size answer is millions of objects every later collection
re-marks. Sessions do not persist — a later session re-imports.
Registry records and the browser manifest are BOTH generated by
`cargo run -p cap-abi --bin gen-manifest` — one source of contracts,
two surfaces. `just test` gates the runner itself (the fixture feed
hash is stable, so registry/fixture drift fails the gate).
