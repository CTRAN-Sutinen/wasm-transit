# Offline apps — every generated UI works offline, and queues its writes

Every generated UI works offline, or fails gracefully (D-189). A page
needs the server at moments: to load the published documents, to
publish a pick, to write the stops document. Between those moments,
the server being away does not stop the work. The server stays the
authority on writes. A write that cannot reach it waits on the device
and is sent when the server answers again, through the same receive
path. Ride-now and the stop display already install and boot offline
(D-65; `docs/architecture/updates-offline.md`). Service design,
service change, and the stops app join them.

## Scope

1. **Offline and updates for the three staff pages.** Each one's
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
3. **Each write through the outbox.**
   - The edit document: its merge is per record, last writer wins.
   - The stops document and stop requests: the outbox keeps the change
     (the module op and its arguments). At send time, the page reads
     the current head and runs the op again over it, so the module's
     checks run on what the server holds.
   - A publish (`POST /plans`) runs at send time, against the set as
     it is then.
   - Work orders.
   - Photos and drawings: kept on the device, and sent before the
     write that names them.

   A refusal at send time (a pick now dated in the past, a change the
   module refuses, a write the person's role cannot make) keeps the
   work and shows it to the person with the server's reasons. Nothing
   is dropped without a word.
4. **The docs.** `docs/architecture/updates-offline.md` and
   `docs/architecture/web.md` describe the outbox and the five offline
   pages. The guide says what a person sees offline.

## Done when

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
