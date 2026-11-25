# Each name declared once

A capability is its crate, and a kind is its line in the kind list. The
generator, the server's assembly, and the engine's op map find each one
there, and two arch gates hold it (`docs/architecture/bus.md`, "Each
name is declared once").

What is left is the proof that a new one costs nothing elsewhere: a
test capability and a test kind, declared in the fixtures, that the
generator, the kind table, and the gates take with no other file
changed. Today the generator reads the kind list as the static `KINDS`
and the capabilities as gen-manifest's generated list, so the test
needs both passed in as parameters.

## Done when

- A test capability and a test kind, added in fixtures, pass the
  generator, the kind table's checks, and the arch gates, with no other
  file changed.
- `just test` passes.
