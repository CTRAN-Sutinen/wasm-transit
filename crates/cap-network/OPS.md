# network

The network codec: read a GTFS feed into an authored network document (the planner's bootstrap — the agency's trips by id with their own times, patterns, service types, blocks; what cannot cross is reported) and export one back to GTFS zip bytes (the publish path). The second capability extracted from the monolith — the codec lives in the shared transit-codec library, and the monolith keeps no copy. Assemble for planners: it is how a planner's session starts and how it publishes.

## import

The planner's bootstrap: read a loaded feed into an authored network document — the feed's trips ARE the document's trips (the agency's ids, every scheduled time, frequency trips as frequency trips), the feed's shapes ARE its alignments (the feed's ids, every point and published distance), patterns are stop sequences along an alignment. Deterministic; what the model cannot hold yet is reported in the notes — never silent. The boundary is the feed's serialization residue (its stop_sequence and shape_pt_sequence numbering, a frequency trip's stop_times base). The document and its boundary register themselves (network_hash, boundary_hash), so the hashes answer validate/export immediately — and, in a linked module, every capability that reads them. `timezone` is the zone the network's dates are counted in (its operators' one agency_timezone; absent when they name none or disagree): what a host turns its clock into the agency's date with. `first_service_date` ("YYYY-MM-DD") is the earliest date any service type with trips runs, absent when nothing runs: the date the import picks on (D-140).

## validate

Every issue of a registered document — the authoring UI's feedback loop, the whole list, never thrown. issues gate (export refuses an invalid document; a block trip its pattern trips do not run is one — the runcut and the publish path would disagree). warnings never gate: they name what validity lets through — trips no block carries (unassigned_trips, once the document has a runcut) and a versioned plan with no service in its first week (no_service_in_plan_period {plan_name, effective_from} — the plan by its name: the calendars a fork inherited end before its pick date).

## export

The publish path: write a registered document as a GTFS zip (deterministic bytes) — every trip under its own id with its own times, the calendar as authored, numbered by its import's boundary when boundary_hash names it (the feed round-trips; 1..n otherwise) — and return it base64-encoded under the content hash of those bytes, the same identity any capability returns for the same bytes. Invalid documents are refused.

## document

A registered document's canonical bytes — its serialization, the bytes its network_hash names — with that hash, its plan frame, and its `timezone` (as import answers it). What a host stores when it persists a network it was sent in any serialization (the sync server publishing a pick): the module is the hash authority, the host stores exactly these bytes and never reads them.

## publishable

What refuses a registered network as a published pick: its validation issues, and the warnings that refuse publication — refusals: a pick that runs no service in its period would hand every rider an empty schedule on its date (D-76). The remaining warnings ride along in warnings and never refuse. The sync server asks it before POST /plans stores a pick.
