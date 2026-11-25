# The prose, at the narrowest scope

A fact lives at the narrowest scope that it covers
(`docs/architecture/README.md`; `docs/CONVENTIONS.md`, "What to
write"). The architecture docs still hold detail that one file, one
route, or one generated list covers. This unit runs after
`names-declared-once.md` and `shapes-from-rust.md`, because it
describes what they build.

## Scope

1. **Route behavior into the overlay.** `docs/architecture/server.md`,
   "The routes", describes each route's behavior. What a caller sees
   moves into the operation's description in
   `server/openapi.overlay.json`. What only the code sees moves to the
   comment of the Go file that does it. The section keeps what spans
   routes, and points to the contract.
2. **The manifest's fields.** 13 of its 25 fields have no description
   in the contract. Write them in the overlay, and replace server.md's
   field-by-field `/manifest` list with a pointer.
3. **One file's design, beside its file.** A section of
   `docs/architecture/` that describes one file (its heading names the
   file, such as server.md's `attachments.go`, `config.go`, and
   `authz.go` sections, or web.md's "The engine") moves to that file's
   header, and the section goes. What spans files stays.
4. **Generated lists, not hand lists.** `docs/FORMATS.md`,
   `docs/architecture/server.md`, and `storage.md` list the kinds by
   hand, and some docs tell a reader to list a capability or an op by
   hand (the justfile's comments, the header of `tools/caps-run.mjs`,
   `storage.md`, `FORMATS.md`). Each points to the kind table, the
   manifest, or the registry's docs site instead.
5. **The built state.** The doc-sweep skill, `crates/AGENTS.md`,
   `docs/guide/first-change.md`, `docs/TESTING.md`, `bus.md`
   ("Answers": the refusal codes), `web.md` (recovery by code),
   `server.md` (the one assembly, its instances, and the coded answer),
   `registry.md`, `release-artifacts.md` ("The HTTP contract":
   generated, and where its prose lives), the justfile's comments on
   `caps` and `http-lock`, `DEPLOYMENTS-SERVER.md` (the one flag), and
   `AGENTS.md`'s row for the HTTP contract describe what the other two
   units built. `docs/CONVENTIONS.md` ("Gotchas") says that a host
   never branches on an error's text.
6. **The writing rules in the developer documents.** Apply "Use these
   words" and "The faults to remove" to `AGENTS.md`,
   `crates/AGENTS.md`, `docs/` (not `docs/decisions/`, which hold the
   user's words), and the `OPS.md` files. A changed `OPS.md` needs
   `just caps-fast`.

## Done when

- No architecture section describes a single file or a single route.
- No doc lists the kinds, the capabilities, or the ops by hand.
- No developer document uses a word in the "Do not write" column of
  "Use these words".
- The docs-site test and `just test` pass.
