# Architecture — the authoritative reference

This directory is the ONE authoritative architecture reference, one
file per topic. The compact tree and ownership rules live in the root
`AGENTS.md`; per-crate, server, web, and bus detail lives here and
nowhere else. The three architecture laws (the network is the core
data model; nobody reinvents the monolith inside a module; one NEED per
module, personas assemble at wire time) are spelled out in
`crates/AGENTS.md`; that file and these are updated together. These
files describe the tree as it is; open work lives in `docs/units/`,
one file per unit.

## The topics

| File | What it covers |
|---|---|
| [crates.md](crates.md) | Every crate, bottom up: the model, the codecs, the libraries, the kernel, the capabilities, the generated assemblies |
| [bus.md](bus.md) | The capability bus: one flat ABI, documents by hash, linked assemblies, and its load-bearing rules |
| [server.md](server.md) | The Go host: its routes, the published documents, the store, auth, publishing picks |
| [static-deployment.md](static-deployment.md) | A rider UI deployed with no server: the export, the bundle, following a GTFS URL |
| [web.md](web.md) | The frontend: parity, the bus worker, external services |
| [generated-uis.md](generated-uis.md) | The generated UIs and the procedure a new one follows |
| [updates-offline.md](updates-offline.md) | How an open page follows the deployment, applies a new build, and works offline |
| [adjacent-systems.md](adjacent-systems.md) | CAD/AVL, workforce, scheduling: what they read and write |
| [registry.md](registry.md) | The capability registry and the agent runner |
| [release-artifacts.md](release-artifacts.md) | Reproducible builds, retained builds, rollback |

Each topic file says what the tree does, in the present tense. Where
the reason for a design is not obvious from what it does, the topic's
**Why** section holds it, including the alternatives that were
considered.

## Deployment shapes

Two ways to deploy are first-class, and the operator chooses per
deployment:

- **With the server** (`just serve`; [server.md](server.md)): the Go
  host imports the agency's feed and publishes the network, streets,
  and walk table; it adds the shared state only a server can hold —
  the edit document (detours, cancels), published picks and their
  effective dates, the geocoder proxy, auth, and the durable store.
- **Without a server** ([static-deployment.md](static-deployment.md)):
  a rider UI exported as static files for any host, its schedule built
  in or followed from the agency's GTFS URL.

Neither shape is the other's subset to be trimmed. A rider UI that runs
without a server keeps running without one, and the server's features
stay the server's: a change made for one shape adds to it and takes no
capability from the other. Both read the same documents through the
same layout (`manifest`, `blobs/<hash>`), so the page code is one.

## Each UI is its own product

Every generated UI ([generated-uis.md](generated-uis.md)) is an
independently deployable product, not a part of one app. The sync
server serves every UI it hosts from one build (`web/dist`, the picker
at `/`), but nothing ties their deployments together: each UI builds
alone (`just build-ui <id>` → `web/dist-ui/<id>/`) and deploys,
updates, and rolls back on its own.

| UI | Without a server | Needs the server for |
|---|---|---|
| ride-now | Yes | Detours and cancels, address search, picks dated ahead |
| stop-display | Yes | Detours and cancels, picks dated ahead |
| service-design | No | Loading the published network, publishing picks |
