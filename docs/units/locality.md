# Locality: find the code a contract names

The contracts are explicit, but the code that carries one write or one
read is spread out. A stop request's write crosses about 12 files in
three languages: the page and its outbox, `server/main.go`,
`logkinds.go`, `caplog.go`, the wasm host, cap-log and transit-log,
cap-requests and transit-requests, the kind's line in
`crates/cap-abi/src/kinds.rs`, and `server/store/log.go`. Four of those
hops are the architecture (page, server, module, store). The rest is
spread that a newcomer must discover. The goal: a person who knows a
kind, an op, or a gate reaches its code in one step. No contract gets
weaker, and no behavior changes.

## Scope, in order

1. **A trace of a kind, from the kind table.** The server registers
   a kind's routes generically from the kind table, so grepping a
   route in the Go code finds no handler. Add `caps-run kind <tag>`
   beside `caps-run tools`. It reads `web/pkg/kinds.json` and prints
   the kind's whole path: its routes and the Go handler that serves
   each, its log, its load op, its check op and the library function
   behind it, its merge, its read rights, and its store layout. A
   kind with no route says so. The generic handlers' file comments
   name the command.
2. **The server in packages, by concept.** `package main` holds 28
   files. The log is the worst case: `logkinds.go`, `caplog.go`,
   `planslog.go`, and `store/log.go` share one concept, and their
   names do not say which part each holds. Move the logs into
   `server/logs/`, with one exported entry point for a write, a fold,
   and a read. Then plans, and the kind table's routes, by the same
   pattern. `store/`, `wasmhost/`, and `httpcontract/` already follow
   it. `main` keeps the wiring, the flags, and the mux.
3. **The gates out of the justfile.** `_host-tests` is 657 lines of
   bash, with 21 inline `node -e` and `python3 -c` scripts. Each gate
   becomes a named script beside the code it guards, or in
   `tools/gates/` when it spans the repo, with its broken fixture
   beside it. The recipe lists the scripts in order. Each failure
   message names its script.

## Done when

- `caps-run kind <tag>` prints the path of every kind in the kind
  table, and a test fails when a printed handler or op does not
  exist.
- `server/logs/` holds the log code, and `package main` holds no log
  logic. Plans and the kind routes are in their own packages. The Go
  tests and `just e2e` pass unchanged, apart from the imports.
- `_host-tests` holds no inline script. Each gate's broken fixture
  still fails it.
- `docs/architecture/server.md`, `docs/TESTING.md`,
  `docs/guide/first-change.md` ("When the build fails"), and the
  `tools/caps-run.mjs` header describe the new homes.
