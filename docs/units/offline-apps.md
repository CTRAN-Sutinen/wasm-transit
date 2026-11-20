# Offline apps — every generated UI works offline, and queues its writes

Every generated UI works offline, or fails gracefully (D-189). A page
needs the server at moments: to load the published documents, to
publish a pick, to write the stops document. Between those moments,
the server being away does not stop the work. The server stays the
authority on writes. A write that cannot reach it waits on the device
and is sent when the server answers again, through the same receive
path. Ride-now and the stop display already install and boot offline
(D-65; `docs/architecture/updates-offline.md`). Service design,
service change, the stops app, and the disruptions app join them.

The first need is a short outage: minutes, or an hour. A person keeps
working, nothing they saved is lost or written twice, and the page
says what waits. Today a staff page keeps its last good documents
until it reloads, and a reload loses them. A failed save is an error
that the person must repeat. A save whose answer is lost and is then
repeated writes twice (a note gets a new id each time). An alert
save after any other alert changed is a `409`.

## Scope

1. **Offline and updates for the four staff pages.** Each one's
   `ui.json` says `"offline": true`. The worker precaches its shell and
   linked assembly, and answers the published documents from their
   last copy. The page shows the date of the data it works from. The
   pages join the update cycle (D-63): a new build waits, and the page
   shows "A new version is ready" and applies it when the person
   accepts, never over unsaved work. A version refusal makes the page
   look for the deployment's build at once (`web/src/versions.ts`).
2. **The outbox, in the kit.** A write that cannot reach the server
   waits in the device's document store, in order. The page always
   shows how many writes wait. They are sent in order when the server
   answers, and a queued write waits for a valid sign-in (an expired
   token holds the outbox, and does not fail it).
   The page's held documents are kept on the device by hash, so a
   reload during an outage boots on them, with the time the server
   last confirmed them.
3. **Every write is resent safely.**
   - The page makes an idempotency key when the person saves, and
     sends the same key on every attempt. The log engine already
     answers a key that landed with its entry (`server/caplog.go`,
     `keyLanded`). Today only `POST /logs/stops` takes a key.
   - A write to a log sends its intent, `{key, ops: [{cap, op,
     args}]}`, as `POST /logs/stops` does today. The server runs each
     op over the head it holds, so the module's checks run on what the
     server holds. One generic write path for every log replaces each
     kind's own handler (the kind registration work).
   - A write is checked against the records it changes, not the whole
     document's head. A change to another record during the outage
     does not refuse it. A change to the same record is a `409` for a
     person to settle.
   - A `503` with `Retry-After` (the bucket not answering, D-119) and
     a network failure both keep the write waiting. A `422` and a
     `409` keep it and show it.
   - A write that waited carries the device's time it was made, as a
     claim beside the server's stamp (D-222).
4. **Each write through the outbox.**
   - The edit document: its merge is per record, last writer wins.
   - The stops document and stop requests: the outbox keeps the change
     (the module op and its arguments), and sends it as its intent.
   - The service alerts and the notes (the disruptions app): each
     write is its intent, and a note keeps the id it was made with.
   - A publish (`POST /plans`) runs at send time, against the set as
     it is then.
   - Work orders.
   - Photos and drawings: kept on the device, and sent before the
     write that names them.

   A refusal at send time (a pick now dated in the past, a change the
   module refuses, a write the person's role cannot make) keeps the
   work and shows it to the person with the server's reasons. Nothing
   is dropped without a word.
5. **The docs.** `docs/architecture/updates-offline.md` and
   `docs/architecture/web.md` describe the outbox and the six offline
   pages. `docs/architecture/storage.md` describes the generic log
   write and the per-record check. The guide says what a person sees offline.

## Done when

- Go: a write whose answer is lost and that is resent with its key
  lands once.
- Go: a write to one record lands after another writer changed a
  different record of the same document.
- e2e, on the disruptions app: the server stops for a few minutes, a
  note and an alert saved meanwhile wait and are each sent once when
  it answers, and a reload meanwhile boots on the held documents.
- e2e, for each staff page: it boots offline from the worker, a write
  made offline waits in the outbox and is sent when the server answers
  again, and a write refused at send time is kept and shown with its
  reasons.
- e2e: a queued stops change whose head moved meanwhile is run again
  over the new head.
- e2e: a staff page refused for its version gets the deployment's
  build, and applies it only when the person accepts.
- A dogfood session on the C-TRAN feed: a planner works through a
  server outage and syncs when it ends.
