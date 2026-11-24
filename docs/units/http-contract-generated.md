# The HTTP contract, generated

`server/openapi.json` is the HTTP contract: 42 paths, every route the
mux serves, with each method's parameters, bodies, status codes, and a
description (`docs/architecture/release-artifacts.md`, "The HTTP
contract"). Go tests hold it to the real server, and the lock holds
each change to expand, then contract. It is written by hand, so every
change to a route edits it, and each body shape is written twice: once
in the code that makes it, and once in the contract. Its whole-document
bodies already name their kind (`x-document`) and do not repeat the
document's schema.

The goal: the contract is generated from the parts that own each fact.
Only the prose (what an operation does, what each status code means) is
written by hand, in one overlay file. A gate in `just test` fails when
the checked-in contract differs from the generator's output. The
contract tests and the lock stay as they are.

## Scope, in order

1. **The overlay.** One hand-written JSON file beside the contract
   (`server/openapi.overlay.json`): each operation's summary and
   description, and the meaning of each status code it answers. JSON,
   so no new dependency. Move the descriptions out of today's contract
   into it.
2. **The routes.** The generator takes the route list from the mux, as
   `TestEveryRouteIsInTheContract` reads it today (it parses
   `server/main.go`). A route the overlay does not describe stops the
   generator.
3. **The document bodies.** A body that is a whole document comes from
   the kind table and the locked schemas (`x-document`). The kind
   table's `route` names a kind's writer in words today ("POST /plans;
   the server's import"). Give it a structured route that the generator
   reads, and keep the words.
4. **The wrapper shapes.** The answers around a document (`from`, the
   hashes, `refusals`, `issues`, `landed`, `seq`) come from their owner,
   as D-249 decides. Blocked: D-249. The answers that Go composes
   itself (`/readyz`, `/manifest`, the store's outage, the pointer
   answers) come from the source D-249 names for them.
5. **The gate.** `just test` runs the generator and fails when its
   output differs from the checked-in `server/openapi.json`, as
   gen-manifest refuses a stale kind table. The contract names its
   generator and says not to edit it (`docs/CONVENTIONS.md`,
   "Traceable").
6. **The first run changes nothing.** The first generated contract is
   compatible with today's lock, with no change to the lock, and the
   contract transport passes over a whole run.
7. **The manifest's fields described.** 13 of the manifest's 25 fields
   have no description in the contract. Write them in the overlay, then
   drop the field-by-field meaning list in
   `docs/architecture/server.md` (`/manifest`) and point to the
   contract.
8. **The docs.** `release-artifacts.md` ("The HTTP contract"), the head
   of server.md's HTTP surface, the justfile's comments on `http-lock`
   and the new gate, and AGENTS.md's row for the HTTP contract say that
   the contract is generated and where its prose lives.

## Done when

- `server/openapi.json` is the generator's output, and a hand edit to it
  fails `just test`.
- A new route needs only its handler and its overlay entry. A changed
  body shape needs only the code that makes it.
- `TestEveryRouteIsInTheContract`, the contract transport over a whole
  run, and `TestTheContractIsCompatibleWithTheLock` pass on the
  generated contract, with the lock unchanged by the first run.
- Every manifest field has a description, and server.md's `/manifest`
  entry points to the contract for them.
- `just test` passes.
