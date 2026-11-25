# Refusals a host acts on, by code

Four host paths recover from a module's error by matching its text:

- `web/src/engine.ts` (`unknownHash`) finds an evicted document in
  `unknown <kind> <hash>: <load> first`, and registers it again.
- `web/src/bus.worker.ts` (`UNKNOWN_DOCUMENT`) matches the same
  refusal in the raw outcome bytes, and does not journal it.
- `web/src/versions.ts` (`newerRefusal`) finds `<kind> v<N> is newer
  than this module` or `was written by a newer build`, and starts the
  update to the new build.
- `server/edits.go` (`editsWriteFailed`) answers 409 when the error
  holds `moved on every attempt`.

If someone rewords one of those messages, the host stops recovering,
and no gate fails. The goal: a refusal that a host acts on carries a
stable code, and no host matches an error's text.

## Scope, in order

1. **The shape.** Every error is an object, `{"error": {"message":
   "…"}}`, and a refusal that a host acts on also carries `code` and its
   fields (D-253). One change moves every host, test, worked example,
   and `server/openapi.json` to it. `cap_abi::Refusal` holds the message
   and an optional code, with `From<String>`, so `?` on a string error
   still compiles. Each capability's `dispatch` returns
   `Result<_, Refusal>`. The codes are one Rust enum, exported to TS by
   ts-rs:
   - `unknown_document {kind, hash, load}`, from `Registry::require`;
   - `newer_version {kind, found, reads, ahead}`, from
     `transit_version::refused` and the read one version ahead.
2. **The hosts read the code.** `unknownHash`, `UNKNOWN_DOCUMENT`, and
   `newerRefusal` read it. The bus worker parses an outcome only when
   it starts with `{"error"`. `wasmhost`'s `decode` keeps the code in a
   Go error type, and the server answers a coded refusal as JSON with
   its code. `receiveLog`'s "moved on every attempt" becomes an
   exported sentinel error that `editsWriteFailed` finds with
   `errors.Is`.
3. **The gate.** A web gate and a Go test in `just test` fail when host
   code matches an error's message with a regex, `includes`, or
   `strings.Contains`. Each fails on a broken fixture in its own test.

## Laws first

Write these before the implementation:

- For each worked example in the manifest that names a document by
  hash, run it with that document evicted. The answer is
  `unknown_document` with that kind, hash, and load op.
- A document one version ahead of what the build reads, and one two
  versions ahead, each answer `newer_version` with the kind and both
  versions. `ahead` says which case it is.
- The engine recovers from an eviction, and the page starts the update
  to a newer build, when the message text is changed and the code is
  kept.

## Done when

- None of the four text matches is left, and the gate fails a new one.
- The law tests pass.
- `just check` passes. A person sees no change, so no dogfood session
  is due.
