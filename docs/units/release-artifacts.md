# Release artifacts — change contracts without breaking them

The parts of a deployment ship at different times. The server's
modules, a page's assembly, a display's cached app shell, and an agent
pulling from the registry can each run a different build at the same
moment. Documents get versions and migrations
([document-versioning.md](document-versioning.md)). This unit does
the same for code. Every artifact ever deployed stays addressable and
rebuildable. The contracts between separately deployed parts (op
shapes and the server's HTTP surface) change by expand-then-contract.
Rolling back code is moving a pointer back to bytes that still exist.
Retirement is the tombstone here too: a build that is no longer
deployed is retired, never deleted.

## Scope

- **Op contracts** (Blocked: D-81, the scheme). An op's request and
  response have no version of their own; the kernel version is one
  number for the whole ABI. Example: cap-plans `publish` gained a
  required `today` (2026-11-05), and nothing would catch a host still
  sending the old shape. The browser runs linked assemblies while
  the server runs single-capability modules (D-78),
  so the two can run different builds. The rule is expand, then
  contract: a new field is optional, or the change is a new op; the
  old shape is accepted until no deployed host sends it; then it is
  removed, with the old artifact retained. The gate diffs each op's
  generated request and response schemas against the last published
  registry (the documents' schema emitter, extended to op shapes), and
  a breaking change fails unless it is the contract step of a
  migration.
- **HTTP contracts** (D-81). `/manifest`, `/edits`, `/pointers`,
  `/blobs`, `/plans`, `/plans/retire`, `/builds`, and `/feed` change under the
  same rule, because a display's cached build meets a newer server.
The build side and retention are in place (docs/ARCHITECTURE.md,
"Release artifacts"): the pinned toolchain, reproducible modules gated
by `just reproducible`, churn cut to each change's own modules except
the kernel's closure, and every build kept in the store (D-80) with
rollback as a pointer move (`POST /builds`; pinned by
`web/e2e/deploy-rollback.spec.mjs` and the store's build laws). What is
left is the contracts between the parts those builds run in.

## Order

Op and HTTP contract versioning, over the artifacts as they are: one
assembly per page, single-capability modules on the server, and the
`{"cap": …}` request envelope. A rollback reaches only a build whose
contracts the newer parts still accept, so the gate also bounds how
far back a rollback can go.

## Done when

- The contract gate fails on a deliberately breaking op change, and
  on a breaking HTTP change, each in its own test; an additive change
  passes.
- D-81 is decided and encoded, and `docs/ARCHITECTURE.md`
  states the compatibility policy shared with document-versioning.
