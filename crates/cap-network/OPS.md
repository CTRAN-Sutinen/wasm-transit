# network

The network codec: it reads a GTFS feed into an authored network document, and it exports a network document back to GTFS zip bytes.

The import is the planner's bootstrap. The document holds the agency's trips by id with their own times, patterns, service types, and blocks. The import reports what cannot cross. The export is the publish path.

This is the second capability extracted from the monolith. The codec lives in the shared transit-codec library, and the monolith keeps no copy. Assemble this capability for planners: it is how a planner's session starts and how it publishes.

## import

Read a loaded feed into an authored network document. This is the planner's bootstrap. The import is deterministic.

The feed's parts become the document's parts:

- The feed's trips are the document's trips. They keep the agency's ids and every scheduled time. Frequency trips stay frequency trips.
- The feed's shapes are its alignments. They keep the feed's ids, every point, and every published distance.
- Patterns are stop sequences along an alignment.

The notes report what the model cannot hold yet. The import is never silent about it. The boundary is the feed's serialization residue: its `stop_sequence` and `shape_pt_sequence` numbering, and a frequency trip's `stop_times` base.

The document and its boundary register themselves. The answer carries `network_hash` and `boundary_hash`. So the hashes answer `validate` and `export` immediately. In a linked module, they also answer every capability that reads them.

The answer carries two more fields:

- `timezone`: the zone that the network's dates are counted in. It is the one `agency_timezone` of the network's operators. It is absent when the operators name none or disagree. A host turns its clock into the agency's date with it.
- `first_service_date` ("YYYY-MM-DD"): the earliest date on which any service type with trips runs. It is absent when nothing runs. It is the date that the import picks on.

## validate

List every issue of a registered document. This is the authoring UI's feedback loop. The op answers the whole list, and it never throws.

Issues gate. `export` refuses an invalid document. For example, a block trip that its pattern trips do not run is an issue, because the runcut and the publish path would disagree.

Warnings never gate. They name what validity lets through:

- `unassigned_trips`: trips that no block carries. It appears once the document has a runcut.
- `no_service_in_plan_period {plan_name, effective_from}`: a versioned plan with no service in its first week. It names the plan by its name. The calendars that a fork inherited end before its pick date.

## export

Write a registered document as a GTFS zip, with deterministic bytes. This is the publish path.

The zip holds every trip under its own id with its own times. It holds the calendar as authored. The numbering follows the import's boundary when `boundary_hash` names it, so the feed round-trips. Otherwise the numbering is 1..n.

The answer returns the zip base64-encoded, under the content hash of those bytes. Any capability returns the same identity for the same bytes. The op refuses invalid documents.

## document

Return a registered document's canonical bytes. These are its serialization, and they are the bytes that its `network_hash` names. The answer also carries that hash, the document's plan frame, and its `timezone` (as `import` answers it).

A host stores these bytes when it persists a network that it was sent in any serialization. An example is the sync server that publishes a pick. The module is the hash authority. The host stores exactly these bytes and never reads them.

## publishable

Answer what refuses a registered network as a published pick. The answer has two parts:

- its validation issues;
- `refusals`: the warnings that refuse publication. A pick that runs no service in its period would hand every rider an empty schedule on its date.

The remaining warnings ride along in `warnings` and never refuse. The sync server asks this op before `POST /plans` stores a pick.

## stored_plans

Read back a plan set that a page stored, and answer its plans by store id. Each plan comes back as stored: the page reads each plan's own version when it uses the plan.

- A tagged set (`"kind": "plan_set"`) in the version this build reads answers its `plans`.
- A set written before its tag is a bare map of plans by store id, and reads as the set.
- A set of another kind or version is refused, and the refusal names both. The page then never writes over it.
