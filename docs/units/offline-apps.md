# Offline apps — the staff pages through an outage, dogfooded on C-TRAN

Every generated UI installs and works offline, and a staff page's
writes wait in its outbox and are sent when the server answers (D-189,
D-222; `docs/architecture/updates-offline.md`, "The outbox"; the intent
write is `docs/architecture/storage.md`). It is on the bus, read by
each staff page, and pinned by Go, the outbox's laws, and
`e2e/offline-staff.spec.mjs`. It is not yet dogfooded on real data
(D-196).

## Scope

A dogfood session on the C-TRAN feed, run as a person doing QA would
run it, in the built pages under the service worker, on a store-mode
server:

- A planner forks a pick in service change, works through an outage
  (the server stopped, not only the browser's network), reloads
  during it, publishes, and sees the publish sent when the server is
  back.
- A dispatcher writes a note and an alert on the disruptions page
  through the same outage.
- A field person surveys a stop with a photo with no signal, and the
  photo and the survey are sent in order.

The session's findings go to `docs/QA-FINDINGS.md`, and the session to
each page's `ui.json`.

## Done when

- The dogfood session above, recorded in each page's `ui.json`
  (`session`), with its findings filed or fixed.
