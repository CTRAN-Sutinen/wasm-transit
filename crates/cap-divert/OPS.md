# divert

Operator-edit authoring: what does this operator edit amount to? An edit is a diversion or a whole-trip cancel.

A diversion draft holds these parts: a route, the pattern anchored against, two kept stops, the interior stops that the detour still serves, a traced path, added minutes, and dates. The draft is answered over the network document. The answer holds these parts:

- the route's patterns and the default;
- each stop's role;
- the skips in effect;
- the alignment that the detour is drawn against;
- the traced path and its length;
- the problems that keep the draft from being a record.

A cancel names a trip on one date its service runs. It names one instance or every instance. The day views take the trip out. Boards list it as cancelled, and plans never ride it.

`propose`, `cancel`, `retire`, and `restore` answer the edit document's next state under the envelope rules: the rev is bumped, and the host clock is stamped. Retirement is the tombstone.

Every op takes the edit document whole as `edits` and answers the next one, tagged. So a host persists or posts the answer as given. An older edit document is refused, and the error names the migration op. A newer one is refused, and the error names both versions. Reading a received document is the sync capability's `read_edits`.

The host owns the documents, the clock, and the record ids.

`reports` answers what each record amounts to on the network: valid, or what is wrong, and the trip instances it touches. Invalid records are reported and never rejected.

Converging two documents is the sync capability's question.

Assemble this capability for anyone who authors operator edits: a dispatcher's UI, a script, or an agent.

## draft

The editor's view of a draft. The op is total: a draft that names what the network lacks answers `problems`, never an error.

The answer holds these parts:

- the route's patterns, each as `{pattern, name, stop_count}`;
- the pattern in effect. It is the draft's pattern when that pattern is the route's. Otherwise it is the longest pattern;
- every visit with its role, and whether it is skipped;
- the skips that the record would carry;
- the pattern's alignment: the authored path, else stop-to-stop;
- the traced slices, flattened (repeated points are dropped), with their length in metres;
- `problems`. The draft is ready when `problems` is empty.

A visit's role is `first_kept`, `last_kept`, `interior`, or `outside`. The anchors are the first visit of the first kept stop and the first visit of the last kept stop after it. A visit is skipped when it is an interior stop that is not in `served_stops`.

## list

The edit document's rows as the list shows them, each in document order. The answer has two lists.

- `rows`: the diversions. Each row holds:
  - the route, the anchor, and the skipped-stop names, resolved over the network (an id that the network lacks shows as itself);
  - the status and `in_force` (retired is the tombstone);
  - the added seconds;
  - whether a detour is drawn;
  - the dates.
- `cancels`: the trip cancels. Each cancel holds:
  - the trip;
  - the instance: the one named, else the trip's only start. It is null for every instance of a frequency trip;
  - its route, and its first and last stops by name;
  - the status and `in_force`;
  - the date.

## reports

What each record of the edit document amounts to on the network, in document order.

For each record, the answer says one of two things. The record is valid against the network, or the answer says what is wrong. The answer also gives how many trip instances the record touches.

A record is valid in these cases:

- A diversion is valid when its route, pattern, and stops exist and are in order.
- A cancel is valid when its trip exists and runs the instance it names.

Invalid records are reported and never rejected (the sync contract). A day view finds nothing to patch for such a record.

## propose

The edit document with the draft proposed as `record_id`. The op stamps `now_ms`.

- A new id appends a draft at rev 1.
- A known id is replaced whole, and its rev is bumped. Its domain id is kept.

The record holds these parts: the anchors, the skips in effect, the detour when the trace has two points or more, `added_mins` rounded to seconds, and the dates.

A draft with problems is refused, and the error names them.

## cancel

The edit document with `trip_id` cancelled on `date` as `record_id`. The op stamps `now_ms`.

- `start_secs` names one instance. Without it, every instance that the trip runs that day is cancelled.
- A new id appends a draft at rev 1.
- A known id is replaced whole, and its rev is bumped. Its domain id is kept. When there is none, the domain id is minted as `cancel-{trip_id}-{now_ms}`.

The day views read the cancel. The trip does not run. Every board lists its visits among `skipped` with the cancel's id. The router never rides the trip.

The op refuses in these cases, and the error names the reason:

- an unknown trip;
- an instance that the trip never runs;
- a date on which its service does not run.

Partial cancel, cover, hold, and swap are not edits yet.

## retire

The edit document with `record_id` retired. This is the tombstone: the record stays, so sync carries the deletion. The rev is bumped and `now_ms` is stamped.

Retiring a retired record answers the same document. An unknown id is an error.

## restore

The edit document with `record_id` back in force as a draft. The next sync submits it again. The rev is bumped and `now_ms` is stamped.

Restoring an in-force record answers the same document.
