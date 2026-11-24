# Refusals a host acts on, by code

Every module error is one string (`{"error": "<message>"}`,
`cap_abi::outcome`). Four places in the hosts act on an error by
matching its text:

- `web/src/engine.ts` (`unknownHash`) finds an evicted document in
  `unknown <kind> <hash>: <load> first`, and registers it again. This
  is how a page recovers after an eviction.
- `web/src/bus.worker.ts` (`UNKNOWN_DOCUMENT`) matches the same refusal
  in the raw outcome bytes, and does not journal it.
- `web/src/versions.ts` (`newerRefusal`) finds `<kind> v<N> is newer
  than this module` or `was written by a newer build`, and starts the
  update to the new build. It reads every module error from the engine,
  and every refused write's body (`signin.svelte.ts`).
- `server/edits.go` (`editsWriteFailed`) answers 409 when the error
  text holds `moved on every attempt` (`receiveLog`).

If someone changes the wording of one of these messages, the host
stops recovering, and no gate fails. The goal: each refusal that a
host acts on carries a stable code and its fields beside its message.
Hosts branch on the code. The message stays text for a person, shown as
given.

Out of scope: every other error stays a string, and a host shows it as
given. The libraries keep their own error types. Only the seam gets
codes, and only for refusals that a host acts on.

## Scope, in order

1. **The shape.** The proposed shape is additive. The outcome keeps
   `"error": "<message>"`, and a coded refusal also has
   `"refusal": {"code": "<code>", ...its fields}`. A host that reads
   only `error` keeps working. The other choice, an `error` that becomes
   an object, changes every host and every worked example in one step.
   The shape is not decided yet: record it as an entry in
   `docs/DECISIONS.md` before item 2 starts.
2. **The codes, in Rust.** Add `cap_abi::Refusal`: the message, and an
   optional code. It has `From<String>`, so `?` on a string error still
   compiles. Each capability's `dispatch` returns `Result<_, Refusal>`,
   and `outcome` writes the code. The codes are one Rust enum, exported
   to TS by ts-rs:
   - `unknown_document {kind, hash, load}`, from `Registry::require`;
   - `newer_version {kind, found, reads, ahead}`, from
     `transit_version::refused` and the read one version ahead.
3. **The web.** `unknownHash`, `UNKNOWN_DOCUMENT`, and `newerRefusal`
   read the code. The bus worker parses the outcome only when it starts
   with `{"error"`, and does not use a regex on the bytes.
4. **The server.** `wasmhost`'s `decode` keeps the code in a Go error
   type. The server answers a coded refusal as JSON with its code, so a
   refused write's body carries it to `versions.ts`. `server/openapi.json`
   gets the field as an expansion only, so the lock holds. Where its
   schema comes from follows D-249.
5. **Go's own refusal.** `receiveLog`'s "moved on every attempt"
   becomes an exported sentinel error, and `editsWriteFailed` finds it
   with `errors.Is`.
6. **The gate.** A web gate and a Go test in `just test` fail when host
   code matches an error's message with a regex, `includes`, or
   `strings.Contains`.
7. **The docs.** `docs/architecture/bus.md` ("Answers") gives the
   shape and the codes. `docs/architecture/web.md` says that the engine
   recovers by the code. `docs/architecture/server.md` gives the coded
   answer. `docs/CONVENTIONS.md` ("Gotchas") says that a host never
   branches on an error's text.

## Laws first

Write these before the implementation:

- For each worked example in the manifest that names a document by
  hash: run it with that document evicted. The answer is
  `unknown_document` with that kind, hash, and load op.
- A document one version ahead of what the build reads, and one two
  versions ahead, each answer `newer_version` with the kind and both
  versions. `ahead` says which of the two cases it is.
- The engine recovers from an eviction, and the page starts the update
  to a newer build, when the message text is changed and the code is
  kept.

## Done when

- None of the four text matches is left, and the gate fails a new one
  (it fails on a broken fixture in its own test).
- The law tests pass.
- `just check` passes. A person sees no change, so no dogfood session
  is due.
- The docs in item 7 describe the codes in the present tense.
