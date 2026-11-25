# Each name declared once

A capability is its crate: the `Request` enum, the dispatch, `OPS.md`,
its worked examples (`src/examples.rs`, behind its `examples` feature),
and its declaration (`capability!`). A kind is one line in the kind
list (`crates/cap-abi/src/kinds.rs`), and its library loads it
(`transit_version::Load`, D-239). The generator takes the capabilities
as data, and gen-manifest is generated (`docs/architecture/bus.md`,
"The declaration" and "gen-manifest"). The server links one assembly
(`docs/architecture/server.md`), and the engine's op map is generated
(`docs/architecture/web.md`, "The engine").

The goal: a new op changes only its capability's crate. A new
capability is a new crate directory. A new kind touches its line, its
library, its capability, and its UI. Nothing else names either, and a
gate holds it.

## Scope

1. **The gate.** Blocked: D-254 (which kinds the server and the engine
   may name). A gate in `just test` fails when a capability's or a
   kind's identifier appears where it does not belong. It matches
   identifiers only (D-244): a kind's load op (`load_notes`), its hash
   field (`notes_hash`), and a capability or kind name as a string
   literal in code. Comments and prose are aside, and so is a kind's
   own route, which the kind table names.
   - A capability may be named only in its crate, the generated
     assemblies, and the fixtures. The generator's code, cap-abi, and
     the root `Cargo.toml` name none. Today the generator names
     network, plans, and streets (the soak check and the walks
     example), and the codec gate's list names three capabilities
     (`CODEC_HOMES`). This half shares the gate with the kind half, so
     it lands with it.
   - The kinds the plumbing may not name follow D-254. Whatever it
     answers, the three uses of `stops` in the server (a pick's export,
     a pick's first date, the feed's differences) become named fields
     in the stops line.
   - The gate fails on a broken fixture in its own test.

## Done when

- An op added to one capability, with its `OPS.md` section and its
  example, changes files in that crate only. A test capability and a
  test kind, added in fixtures, pass with no other file changed.
- No Go function calls more than one op for a write. The work order
  route calls two, around the system's own call (`workorders.go`).
- D-248 (the size that D-239 costs each module) is answered, and the
  release gate it names holds.
- `just test` passes.
