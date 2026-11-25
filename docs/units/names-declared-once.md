# Each name declared once

A capability is its crate: the `Request` enum, the dispatch, `OPS.md`,
its worked examples (`src/examples.rs`, behind its `examples` feature),
and its declaration (`capability!`). A kind is one line in the kind
list (`crates/cap-abi/src/kinds.rs`), and its library loads it
(`transit_version::Load`, D-239). The generator takes the capabilities
as data, and gen-manifest is generated (`docs/architecture/bus.md`,
"The declaration" and "gen-manifest").

The goal: a new op changes only its capability's crate. A new
capability is a new crate directory. A new kind touches its line, its
library, its capability, and its UI. Nothing else names either, and a
gate holds it.

The plumbing still names some of them today:

- The server runs one module for each capability, with one flag each
  (`-wasm`, `-plans-wasm`, `-notes-wasm`, and eight more;
  `server/flags.go`, `server/config.go`). A flow that needs two
  capabilities is composed in Go.
- The engine names each capability in its typed op map
  (`web/src/engine.ts`).
- The stop requests' work order path is in Go
  (`server/workorders.go`).
- A kind's write route is words in the kind table ("POST /plans; the
  server's import").

## Scope, in order

1. **The server links one assembly**, as each UI does (D-227).
   `gen-assemblies` writes `module-server` from one list of the
   capabilities the server hosts, read by gen-assemblies and by the
   server (`docs/CONVENTIONS.md`, "Traceable"). `just caps` builds it,
   and `independence.mjs` checks it like a UI's assembly. The reasons
   for separate modules stay, as instances of the one assembly:
   - authz runs in an instance of its own, so a trap in another
     capability never takes the fail-closed check with it;
   - an import or a publish runs in a fresh instance, so the memory it
     used goes with it;
   - the long-lived instance holds the sync, plans, stops, alerts,
     notes, and log documents, each registered once. A request
     registers nothing it already holds, and evicts every document no
     head names (the kernel's evict, op 257);
   - a refresh runs beside the instance, never in a request's path.
     When the long-lived instance passes its memory ceiling, the host
     starts a fresh instance of the same compiled assembly and
     registers the current heads' documents in it. Before the swap, it
     registers each head that moved during the refresh. The old
     instance finishes the call it holds, then closes. A trap replaces
     the instance the same way. A refresh starts on memory growth,
     never on the number of requests.
2. **One flag.** `-wasm` names the assembly, and the manifest's
   `wasm_hash` names it too. The per-capability flags go, with their
   entries in `server/dev.toml`, `local.example.toml`, the e2e servers,
   and the image (D-244).
3. **The names the plumbing still holds.** The engine's typed op map is
   generated from the manifest's capabilities. The kind table gains a
   structured write route (method and path) beside its words, and the
   server's work order path reads it. Each difference between kinds is
   a named field in the kind's line, never a hook.
4. **The gate.** A gate in `just test` fails when a capability's or a
   kind's identifier appears where it does not belong. It matches
   identifiers only (D-244): a kind's load op (`load_notes`), its hash
   field (`notes_hash`), and a capability or kind name as a string
   literal in code. Comments and prose are aside, and so is a kind's
   own route, which the kind table names.
   - A capability may be named only in its crate, the generated
     assemblies, and the fixtures. The generator's code, cap-abi, and
     the root `Cargo.toml` name none.
   - A kind may be named only in its line, its library, its capability,
     a capability or library that reads it (`capability! { documents:
     [...] }`), its UI, and their tests. The kernel, the server, the
     runner, and the engine name none.
   - The gate fails on a broken fixture in its own test.

## Done when

- The server boots on `module-server` alone, and `go test -race`
  passes. Go tests show that a trap during a write leaves authz
  answering, that an import's instance is gone after the import, and
  that requests during a refresh are answered with no delay and none
  by the old instance after the swap.
- The per-capability flags are gone from the server and every settings
  file.
- An op added to one capability, with its `OPS.md` section and its
  example, changes files in that crate only. A test capability and a
  test kind, added in fixtures, pass with no other file changed.
- No Go function calls more than one op for a write.
- D-248 (the size that D-239 costs each module) is answered, and the
  release gate it names holds.
- `just test` passes.
