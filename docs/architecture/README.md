# Architecture reference

This directory is the architecture reference. It has one file for each
topic, and each topic has only one home. The detail for each crate, the
server, the web, and the bus is here and in no other place.

Related documents:

- The root `AGENTS.md` holds the short crate tree and the ownership
  rules.
- `crates/AGENTS.md` states the three architecture laws. Update it
  together with these files. The laws are:
  1. The network is the core data model.
  2. No module rebuilds the old all-in-one module.
  3. Each module answers one need. The set of modules for a user is
     chosen when the modules are assembled, not when they are designed.
- `docs/units/` holds the open work, one file for each unit. These
  files describe only what the code does now.
- [The guide](../guide/README.md) is the plain-language introduction
  for people. It explains these topics and links here for the detail.

## The topics

| File | What it covers |
|---|---|
| [agency-data.md](agency-data.md) | Who owns the data (the agency). Applications are interfaces. Reads are open, and the receive path governs writes. |
| [crates.md](crates.md) | Every crate, from the bottom up: the model, the codecs, the libraries, the kernel, the capabilities, and the generated assemblies |
| [bus.md](bus.md) | The capability bus: one module interface, documents named by hash, linked assemblies, and the rules the bus depends on |
| [server.md](server.md) | The Go host: its routes, the documents it publishes, sign-in, and publishing picks |
| [storage.md](storage.md) | The agency's record: the bucket, what it holds for each shape of data, and how it is written and read |
| [static-deployment.md](static-deployment.md) | A rider UI deployed with no server: the export, the bundle, and following a GTFS URL |
| [web.md](web.md) | The frontend: parity, the bus worker, and external services |
| [generated-uis.md](generated-uis.md) | The generated UIs, and the procedure for a new one |
| [updates-offline.md](updates-offline.md) | How an open page follows the deployment, applies a new build, and works offline |
| [adjacent-systems.md](adjacent-systems.md) | CAD/AVL, workforce, and scheduling systems: what they read and write |
| [registry.md](registry.md) | The capability registry and the agent runner |
| [release-artifacts.md](release-artifacts.md) | Reproducible builds, kept builds, rollback, the HTTP contract, and each UI at its own URL |

Each topic file says what the code does, in the present tense. When the
reason for a design is not clear from what it does, the topic's **Why**
section gives the reason and the alternatives that were considered.

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
