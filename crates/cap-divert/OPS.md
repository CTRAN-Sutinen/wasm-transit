# divert

Operator-edit authoring: what does this operator edit amount to? An edit is a diversion or a whole-trip cancel (D-45). A diversion draft — a route, the pattern anchored against, two kept stops, the interior stops the detour still serves, a traced path, added minutes, dates — is answered over the NETWORK document (the route's patterns and the default, each stop's role, the skips in effect, the alignment the detour is drawn against, the traced path and its length, and the problems keeping it from being a record); a cancel names a trip — one instance or every instance — on one date its service runs, and the day views take it out (boards list it as cancelled, plans never ride it). propose, cancel, retire, and restore answer the edit document's next state under the envelope rules (rev bumped, host clock stamped; retirement is the tombstone). Every op takes the edit document whole as `edits` and answers the next one tagged, so a host persists or posts the answer as given; an older edit document is refused naming the migration op, a newer one naming both versions (reading a received document is the sync capability's read_edits). The host owns the documents, the clock, and the record ids. reports answers what each record amounts to on the network (valid or what is wrong, the trip instances it touches) — invalid records are reported, never rejected. Converging two documents is the sync capability's question. Assemble for anyone who authors operator edits: a dispatcher's UI, a script, an agent.

## draft

The editor's view of a draft: the route's patterns ({pattern, name, stop_count}), the pattern in effect (the draft's when it is the route's, else the longest), every visit with its role (first_kept / last_kept / interior / outside — the anchors are the first visit of the first kept stop and the first visit of the last kept stop after it) and whether it is skipped (an interior stop not in served_stops), the skips the record would carry, the pattern's alignment (authored path, else stop-to-stop), the traced slices flattened (repeated points dropped) with their length in metres, and problems (empty = ready). Total: a draft naming what the network lacks answers problems, never an error.

## list

The edit document's rows as the list shows them, each in document order: `rows`, the diversions — route, anchor, and skipped-stop names resolved over the network (an id the network lacks shows as itself), status and in_force (retired is the tombstone), added seconds, whether a detour is drawn, the dates; `cancels`, the trip cancels — the trip, the instance (the one named, else the trip's only start; null for every instance of a frequency trip), its route and first and last stops by name, status and in_force, the date.

## reports

What each record of the edit document amounts to on the network, in document order: valid against it (a diversion's route, pattern, and stops exist and are in order; a cancel's trip exists and runs the instance it names) or what is wrong, and how many trip instances it touches. Invalid records are REPORTED, never rejected (the sync contract) — a day view simply finds nothing to patch for one.

## propose

The edit document with the draft proposed as record_id: a new id appends a draft at rev 1, a known id is replaced whole with its rev bumped (its domain id kept); now_ms stamped. The record: the anchors, the skips in effect, the detour when the trace has two points or more, added_mins rounded to seconds, the dates. A draft with problems is refused, naming them.

## cancel

The edit document with trip_id cancelled on date as record_id (D-45): start_secs names one instance, and without it every instance the trip runs that day is cancelled. A new id appends a draft at rev 1, a known id is replaced whole with its rev bumped (its domain id kept, else minted cancel-{trip_id}-{now_ms}); now_ms stamped. The day views read it: the trip does not run, every board lists its visits among `skipped` with the cancel's id, and the router never rides it. Refused, naming the reason: an unknown trip, an instance the trip never runs, a date its service does not run. Partial cancel, cover, hold, and swap are not edits yet.

## retire

The edit document with record_id retired — the tombstone (the record stays so sync carries the deletion), rev bumped, now_ms stamped. Retiring a retired record answers the same document. An unknown id is an error.

## restore

The edit document with record_id back in force as a draft (the next sync re-submits it), rev bumped, now_ms stamped. Restoring an in-force record answers the same document.
