# Every answer's shape from its Rust type

An answer's shape is written more than once today. Rust makes it, ts-rs
exports it to TS, and `server/openapi.json` repeats it by hand (42
paths, held to the server by Go tests). And four host paths recover from a module's error by
matching its text:

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
and no gate fails.

The goal: each shape has one source, its Rust type (D-249). TS, the
manifest, and the HTTP contract are generated from it. A refusal that
a host acts on is one of those shapes, with a stable code, and no host
matches an error's text.

## Scope, in order

1. **Answer schemas.** Each op's answer type derives
   `schemars::JsonSchema`, and gen-manifest writes the schemas beside
   the kind schemas.
2. **Coded refusals.** Every error is an object,
   `{"error": {"message": "…"}}`, and a refusal that a host acts on also
   carries `code` and its fields (D-253). One change moves every host,
   worked example, and the contract to it. `cap_abi::Refusal` holds the
   message and an optional code, with `From<String>`, so `?` on a string
   error still compiles. Each capability's `dispatch`
   returns `Result<_, Refusal>`. The codes are one Rust enum:
   `unknown_document {kind, hash, load}` from `Registry::require`, and
   `newer_version {kind, found, reads, ahead}` from
   `transit_version::refused` and the read one version ahead. Every
   other error stays a string, shown as given.
3. **The hosts read the code.** `unknownHash`, `UNKNOWN_DOCUMENT`, and
   `newerRefusal` read it. The bus worker parses an outcome only when
   it starts with `{"error"`. `wasmhost`'s `decode` keeps the code in a
   Go error type, and the server answers a coded refusal as JSON with
   its code. `receiveLog`'s "moved on every attempt" becomes an
   exported sentinel error that `editsWriteFailed` finds with
   `errors.Is`.
4. **The HTTP contract, generated.** A generator builds
   `server/openapi.json` from:
   - the routes, from the mux, as `TestEveryRouteIsInTheContract` reads
     them;
   - the whole-document bodies, from the kind table and the locked
     schemas (`x-document`);
   - the answer shapes, from step 1;
   - one hand-written overlay, `server/openapi.overlay.json`: each
     operation's summary and description, the meaning of each status
     code, and the schemas of the answers Go composes itself
     (`/readyz`, `/manifest`, the store's outage, the pointer answers).

   A route the overlay does not describe stops the generator. The
   contract names its generator and says not to edit it, and
   `just http-lock` records the generated contract as the lock.
5. **The gates.** `just test` fails when the checked-in contract
   differs from the generator's output. A web gate and a Go test fail
   when host code matches an error's message with a regex, `includes`,
   or `strings.Contains`. Each gate fails on a broken fixture.

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

- `server/openapi.json` is the generator's output, and a hand edit
  fails `just test`. A new route needs only its handler and its overlay
  entry, and a changed answer shape needs only its Rust type.
- `TestEveryRouteIsInTheContract` and the contract transport over a
  whole run pass on the generated contract.
- None of the four text matches is left, and the law tests pass.
- `just check` passes. A person sees no change, so no dogfood session
  is due.
