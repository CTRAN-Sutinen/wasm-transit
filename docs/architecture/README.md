# Architecture reference

This directory holds what spans the system: how the parts fit, the
failure model, and the invariants that cross components. It has one
file for each topic. A fact lives at the narrowest scope that it
covers, so detail that one file, package, or crate covers lives with
that code: a crate's `//!` comment, a Go package or file comment, a TS
file header, a capability's `OPS.md`, or a route's description in
`server/openapi.json`.

Related documents:

- The root `AGENTS.md` holds a map of the repo, which points to the
  home of each part, and the ownership rules.
- `crates/AGENTS.md` states the three architecture rules. Update it
  together with these files.
- `docs/units/` holds the open work, one file for each unit. These
  files describe only what the code does now.
- [The guide](../guide/README.md) is the plain-language introduction
  for people. It explains these topics and links here for the detail.

## The topics

| File | What it covers |
|---|---|
| [agency-data.md](agency-data.md) | Who owns the data (the agency). Applications are interfaces. Reads are open, and the write path governs writes. |
| [crates.md](crates.md) | Where a crate is described: its own `//!` crate comment, and a capability's `OPS.md` |
| [bus.md](bus.md) | The capability bus: one module interface, documents named by hash, linked assemblies, and the rules the bus depends on |
| [hosts.md](hosts.md) | What a host is (an upstream, a working copy, the modules, a clock), the rules every host keeps, and the three ways the hosts differ |
| [server.md](server.md) | The Go host: its routes, the documents it publishes, sign-in, and publishing picks |
| [storage.md](storage.md) | The agency's record: the bucket, what it holds for each shape of data, and how it is written and read |
| [static-deployment.md](static-deployment.md) | A rider UI deployed with no server: the export, the bundle, and following a GTFS URL |
| [web.md](web.md) | The frontend: parity, the bus worker, and external services |
| [generated-uis.md](generated-uis.md) | The generated UIs: the shared kit, the components, the procedure for a new one, and the gates. Each UI has one file in `generated-uis/`, with its failure table |
| [updates-offline.md](updates-offline.md) | How an open page follows the deployment, applies a new build, and works offline |
| [adjacent-systems.md](adjacent-systems.md) | CAD/AVL, workforce, and scheduling systems: what they read and write |
| [registry.md](registry.md) | The capability registry and the agent runner |
| [release-artifacts.md](release-artifacts.md) | Reproducible builds, kept builds, rollback, the HTTP contract, and each UI at its own URL |

Each topic file says what the code does. When the reason for a design is
not clear from what it does, the topic's **Why** section gives the
reason and the alternatives that were considered.

## Deployment shapes

An operator chooses one of two deployment shapes for each deployment.
Both shapes are complete.

- **With the server** (`just serve`; [server.md](server.md)). The Go
  host imports the agency's feed. It publishes the network, the
  streets, and the walk table. It also holds the shared data that only
  a server can hold: the edit document (detours and cancellations),
  published picks and their start dates, the geocoder proxy, sign-in,
  and the durable store.
- **Without a server** ([static-deployment.md](static-deployment.md)).
  A rider UI is exported as static files for any web host. The schedule
  is built into the export, or the pages follow the agency's GTFS URL.

Neither shape is a reduced copy of the other. A rider UI that runs
without a server keeps running without one. The server's features stay
with the server. A change for one shape adds to that shape and removes
nothing from the other. Both shapes serve the same documents at the
same paths (`manifest`, `blobs/<hash>`), so one version of the page
code serves both.

## Built for the outage

A transit agency keeps running when its technology fails, and it has
fallback procedures. Each part of this project is designed for the
outage it will have, not only to be available. For each failure, the
design states what still works, what each person does instead, and how
the part recovers.

- **Every copy but the bucket is a working copy.** A server copy, a
  page's cache, and an exported bundle can each be lost, and nothing
  of the record is lost with them. A server copy that cannot reach the
  bucket serves the documents it last confirmed, labelled stale, and
  refuses writes with `503` ([storage.md](storage.md)). A copy that
  starts during the outage does the same from its disk cache
  ([server.md](server.md), "The disk cache"). Every page
  keeps running offline on the documents it holds, and a staff page's
  writes wait on the device until the server answers
  ([updates-offline.md](updates-offline.md)).
- **The bucket alone recovers the agency.** It holds:
  - the record: the log keeps every state of every record
    ([storage.md](storage.md));
  - how to read the record: the store's layout, a plain README, the
    kind table, and every schema (`docs/FORMATS.md`, "The store
    describes itself");
  - the code that reads it: every module and every UI build, by hash
    ([release-artifacts.md](release-artifacts.md)).

  So there are two ways to recover. Run the stored builds again in any
  wasm host, which gives the same answers as at the time. Or read the
  documents with no software: each one is plain JSON or an open
  format, and it names its kind and version. Neither way needs this
  repo, a build machine, or a vendor.
- **Each UI names what it does without each input.** A page never
  shows an old answer as a current one. The generated-UI procedure
  writes a failure table for each UI
  ([generated-uis.md](generated-uis.md), step 5).

### Why

Availability and resilience are different measures. Availability
counts the time a system is up. Every system has outages, so
resilience asks what happens during one. An agency's own procedures
already answer that for its staff. The software must say what it
still does, so that the staff know when to change to their fallback
procedure, and the work comes back together when the outage ends.

## Each UI is its own product

Each generated UI ([generated-uis.md](generated-uis.md)) is a product
that is deployed on its own. A deployment puts each UI at its own URL.

- In development, the sync server serves every UI from one build
  (`web/dist`, with the picker at `/`). This is `just serve` and
  `just dev`.
- Each UI also builds alone: `just build-ui <id>` writes
  `web/dist-ui/<id>/`.
- Each rider UI exports as its own static bundle (`just export`).
- In a deployment, each UI is at its own URL. Its host serves the UI's
  bundle and routes the data paths to the server. The store keeps each
  UI's builds, and each UI rolls back alone
  ([release-artifacts.md](release-artifacts.md), "Each UI at its own
  URL"; D-171).

| UI | Runs without a server | Needs the server for |
|---|---|---|
| ride-now | Yes | Detours and cancellations, address search, picks dated ahead |
| stop-display | Yes | Detours and cancellations, picks dated ahead |
| service-design | No | Loading the published network, publishing picks |
| service-change | No | Loading the published network, publishing picks |
| stops | No | Reading and saving the stops document |
| disruptions | No | The edit document, the stops, the service alerts, and the notes |
