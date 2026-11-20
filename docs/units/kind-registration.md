# A document kind, declared once

A kind is one type of document the agency keeps, such as stops,
alerts, or notes. Adding a kind costs too much. The notes kind is named
in 13 files outside its tests (2026-11-20):

- in the kernel: `crates/cap-abi/src/kernel.rs`,
  `src/bin/common/kind_table.rs`, and `src/bin/gen_manifest.rs`;
- in the server: `server/notes.go`, `server/logwrite.go`, and
  `server/main.go`;
- in the runner: `tools/caps-run.mjs`;
- in the engine: `web/src/engine.ts`;
- in its page.

Each log kind has its own Go file of about 130 to 400 lines. Every file
holds the same five functions: `withX`, `xLog`, `assembleX`, `xHead`,
and `xLogDoc`. Then it has a handler. `cap-abi` depends on 17 domain
crates, so the kernel is a hub that every new kind edits. Runs, the work
rules, and the policy in logs are next (`storage-model.md`), so this
cost limits each of them.

The goal: a new kind touches its declaration, its library, its
capability, and its UI. Nothing else names it.

## Scope

1. **One declaration for each kind.** The declaration states:
   - the kind's tag and version;
   - its library type;
   - its log, or the fact that it is a document with no log;
   - its routes;
   - the capability that validates its writes.

   The declarations are one explicit list in one file. A kind exists
   only if it has a line in that list. Nothing discovers kinds: no
   link-time registration, and no scan of the crates. The kind table
   (`web/pkg/kinds.json`, D-202) is generated from the list. It is
   checked in, so a reviewer sees a new kind in the diff, and a gate
   fails when it differs from the list.
2. **The kernel takes its kinds from the list.** Each kind's load
   lives in its owning library. The kernel reads the list, not one
   hand-written function for each kind. Each line of the list names
   the library function that loads its kind, so a reader goes from
   the list to the code in one hop.

   Whether `cap-abi` stops depending on the domain crates, and an
   assembly links only the libraries of the kinds its capabilities
   read, changes how the kernel and the assemblies are built. A
   proposal goes to the user first, with what it costs the arch
   gates and the module sizes.
3. **One server path for every log.** One generic route serves each
   log's read, head, fold, and write. It is driven by the kind table.
   The per-kind Go files (`notes.go`, `alerts.go`, `requests.go`, and
   the log parts of `stopslog.go` and `planslog.go`) are deleted. The
   routes and their bytes stay the same. `server/openapi.json`
   compares them. Each error and log line of the generic route names
   its kind.
4. **One reader in the engine and one loader in caps-run.**
   `served(kind)` replaces `servedStops`, `servedNotes`, and the
   others. caps-run's `--load KIND:FILE` reads the kind table.
5. **Generated lists in the docs.** `docs/FORMATS.md`, `AGENTS.md`,
   `docs/architecture/server.md`, and `storage.md` list the kinds by
   hand today. They point to the kind table and the registry's docs
   site instead.
6. **The line holds.** A gate in `just test` fails when a kind's name
   appears outside its declaration, its library, its capability, its
   UI, and their tests.

Every item follows `docs/CONVENTIONS.md`, "Traceable". Where a kind
differs from the others, the difference is a named field in its line,
never a hook.

## Done when

- The gate runs in `just test`. Every kind passes it, the notes kind
  among them.
- The next new kind (or a test kind added and removed in a Go and Rust
  test) lands touching only the places the gate allows.
- `go test` and the openapi compatibility gate pass with the per-kind
  Go files deleted.
- The kind lists in the docs point to the generated sources.
