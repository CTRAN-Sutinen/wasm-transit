# The crates — the layering, bottom up

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
                timed_transfers folded) and its near misses
                (missed_connections: another route leaving just
                before the rider can make it, with that route's next
                departure, D-133), a stop's waits (stop_gaps; a
                station's are its bays', pooled), the network's
                longest waits (longest_waits: every stop ranked, in an
                hour window or over the day, D-134), and coverage of a WEIGHTED LAYER
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
                and THE BOARD AT A MOMENT (stop_board_at,
                grouped_board_at: the date's board and, given the
                previous date at the same instant — its clock's
                seconds and its length, 23/24/25 h — that date's
                departures still to leave: GTFS times past 24:00 are
                the date the trip started on, so at 00:30 Friday's
                24:45 leaves in 15 minutes; each date reads its own
                edits and its own live instances, every row names its
                date, rows order by how long until each leaves, and
                in the hour before a fall-back day's anchor the wait
                counts from the true instant, not the clamped 0),
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
                descriptor's start_time and start_date) over them, and the joins
                over the NETWORK: an rt trip id IS a model trip id,
                the instance its start (a frequency trip's, the
                descriptor's start_time — `resolve_instance`) and its
                service date (the descriptor's start_date; absent,
                none is stated; unreadable, the update names no
                instance). The
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
                (v2: delays per trip instance (trip, service date,
                start) and stop — the date as the feed states it,
                absent meaning the current service day, GTFS-rt's
                reading; `LiveDay::on(date, current)` is one date's
                view, which every reader asks for its date —
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

## Why

One entry per load-bearing choice, headed by the code it governs.
**Holds** is what must stay true; change it only with the user, after
reading **Because**.

### An alignment edit names its reach (author `draw_alignment`, `redraw: one|all`, `touched`)
- **Holds:** redrawing a pattern's line says whether it redraws that pattern alone (a new alignment) or every pattern on the shared one, and answers the patterns it touched.
- **Because:** several patterns may run one alignment; a fix meant for one variant must never move the others silently, and a fix meant for all must not need one edit per pattern.
- **Rejected:** always redraw the shared alignment; always copy it under a new id.
- **Decided:** D-11.

### Timetable gestures act on one service type (author `apply` gestures, `GesturePanel`)
- **Holds:** a gesture changes the trips of the one service type it names; the UI warns when other types run with it. No authored "same as, except" relation.
- **Because:** the reads pool a combination, but its members are separate authored types; acting on all silently would change trips the planner did not pick.
- **Rejected:** gestures acting on the whole family; a "same as, except" relation (a model change, QAF-44).
- **Decided:** D-8.

### A fork carries weekly service through its period (author `set_plan {through}`)
- **Holds:** a fork with a period end extends every weekly service type still running in the calendar's last week to that end; dated-only types and every exception stay as they were.
- **Because:** a fork picked after the import's calendar runs nothing until every type is extended (13 edits on C-TRAN); holidays are next year's to add deliberately.
- **Rejected:** a separate `extend_service_ranges` gesture; leaving it to the planner by hand.
- **Decided:** D-41 (where `through` comes from: D-104; which types: D-112).

### Dated calendars import as weekly rules (transit-codec calendars)
- **Holds:** a type whose added dates are every flagged weekday of a span, but for a few removed dates, imports as days + range + exceptions, and exports back exactly as the feed had it.
- **Because:** C-TRAN writes all service as dated additions, so nothing is "weekly" and a fork would carry nothing; the rhythm is in the dates. Reading it at the codec keeps the model's rule explicit and the export lossless.
- **Rejected:** inferring the rhythm at the fork; nothing inferred (the planner flags days).
- **Decided:** D-105 (recognition: D-111).

### "Frequent" counts departures only (transit-analysis coverage)
- **Holds:** both frequency tests count departures, so a terminal where nobody boards is never frequent.
- **Decided:** D-6. **Pinned:** `a_terminal_nobody_boards_at_is_never_frequent`.

### Coverage takes any weighted layer (`load_layer`, `type_coverage`, `unserved_points`)
- **Holds:** a layer is points or polygons with any numeric weight and a planner's label (population, jobs, …); polygons are areally weighted, and a point layer states its resolution.
- **Because:** a tract's people at its centroid misread rural tracts and swing with the radius; the system is not opinionated about what a planner weights.
- **Rejected:** areal population only; finer point layers only.
- **Decided:** D-47 (reading and sampling: D-97, D-98).

### A service group is a combination that runs together (`service_groups`, `members`)
- **Holds:** a group is the set of service types that run on the same dates, shown with those dates; types that never share a day are never pooled, and no "representative day" stands in for a combination.
- **Because:** pooling a base type with subsets that run in successive periods counted trips that never run together (2,130 against 1,774 on C-TRAN); a planner needs the real combinations and when they hold.
- **Rejected:** counting members on a representative day; splitting a group into periods with representative days; counting every member and saying so.
- **Decided:** D-55.

### Required recovery: the work rules, else the terminal's floor (`recovery`, transit-runs)
- **Holds:** a policy (minutes and/or a share of run time, per route or service type, the work-rule document's `recovery`) wins; the smallest layover seen at that terminal is the fallback.
- **Because:** an imported layover is the whole observed gap, so without a rule every delay reads infeasible.
- **Decided:** D-9 (which waits it judges: D-110).

### Blocking is a suggestion under the work rules (runcut `suggest_blocks`)
- **Holds:** automatic blocking is the start of the runcut, in the runcut capability, tuned by the work rules (recovery, interlining per route pair); its blocks are suggestions the planner adopts or edits, and the peak vehicle need falls out of them.
- **Because:** a vehicle need is a block assignment; a standalone estimate from concurrent trips would answer a number no planner could act on.
- **Rejected:** a peak estimate with no blocks; blocking by hand only.
- **Decided:** D-46 (what a suggestion changes: D-109; interlining with no rule: D-107).

### Work rules are one generic document, every rule optional (`work_rules`, kernel load 269)
- **Holds:** the common rules (platform, spread, report and pull, pieces, meals, relief points, pay, recovery, interlining) live in one policy document, each optional; an agency's values are data.
- **Because:** nothing may couple to one agency's labor agreement.
- **Rejected:** shaping the document to C-TRAN's rules.
- **Decided:** D-50.

### A plan's time is door to door (router `leave_secs`, `duration_secs`)
- **Holds:** an itinerary carries when the rider must leave (the first boarding minus the walk before it) and the trip's length from then; "leave in N min" uses the board's `wait` rule.
- **Because:** that is what riders compare and what other trip planners show; host arithmetic over legs would put an answer outside the module.
- **Rejected:** a duration from the moment asked.
- **Decided:** D-40 (on a plan not asked for now: D-92).

### A stopped route looks ahead up to a week (board `grouped_board`, transit-dispatch next service)
- **Holds:** a group with no departures left shows the next one from the following days, each day patched by its own edits and never the live feed; nothing past seven days.
- **Because:** "Next: Monday 5:12 am" is what a rider at a stopped route needs; one weekly cycle covers every regular pattern.
- **Rejected:** tomorrow only; until the calendar ends.
- **Decided:** D-42.

### Stop search is fuzzy on the stops' own names (directory `stops`)
- **Holds:** typos and abbreviations match the names the feed gives; no authored aliases.
- **Because:** aliases would be a hand-maintained separate dataset; a geocoder covers everyday names where a deployment runs one (D-36).
- **Rejected:** aliases in the network; deferring entirely to a geocoder.
- **Decided:** D-48 (what counts and the order: D-91).

### A cancelled trip is not a row (`TripCancel`, `LiveDay::runs`, the board and the router)
- **Holds:** the edit document carries whole-trip cancels beside diversions; a trip cancelled by a dispatcher shows on boards as cancelled and is never ridden. A trip the live feed cancels or deletes, or a stop it skips, is no row on any board, with no mark and no note, and the router never boards or alights there.
- **Because:** cancel is a dispatcher's first answer to a late trip, so it lands first; partial cancel, cover, hold, and swap each need a meaning on every reader. A live cancellation shown at its scheduled time misled riders; the user chose to drop it silently.
- **Rejected:** the full dispatcher set at once; showing a live cancel marked, or noting it on the group.
- **Decided:** D-45, D-73 (the shape: D-100..D-102).

### Streets: any source, per-mode profiles (transit-streets import, `walks`, `access`, streets `route` `to_confirm`)
- **Holds:** every street op reads a streets document by hash, whatever source filled it. Walking is strict: every class but motorway, ramp, and unknown; one-way does not bind a rider; at most ten minutes at 1.3 m/s by default. A bus may be drawn on any class; unknown and path-like classes are named for the planner to confirm.
- **Because:** the source is data, not model: an agency's roads give scheduling parity, OSM gives sidewalks, and which one a deployment loads is its choice (C-TRAN's roads cannot carry walking, QAF-85). A planner judges a grey-area road; the data cannot.
- **Rejected:** one fixed source for every mode; refusing a bus on unknown classes.
- **Decided:** D-25, D-26 (classes: D-30, D-103).

### The feed's own facts are not inferred (analysis `transfer_stops`, rt `late[].label`)
- **Holds:** a transfer point is the feed's flag, never inferred (RAPTOR finds transfers regardless); a vehicle with no label shows its id.
- **Because:** these are the agency's facts, and the rules stay generic rather than coupled to one agency.
- **Decided:** D-10.
