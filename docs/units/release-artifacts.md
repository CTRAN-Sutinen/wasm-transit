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
  `/blobs`, `/plans`, `/plans/retire`, `/builds`, `/feed`, `/stops`,
  `/stop-requests`, and `/schemas` change under the same rule, because
  a display's cached build meets a newer server.
- **Each UI at its own URL.** A deployment puts every UI at a URL of
  its own, deployed, updated, and rolled back alone; the server's one
  build at `/` with the picker (one `build` pointer for the whole
  `-dist`, one service worker at the root controlling every page) is
  the development setup. What is left:
  - **A bundle for every UI.** `just export` makes one for the rider
    UIs; the staff UIs (service design, service change, stops) need
    the server's data and writes, so their bundle carries no
    published documents and reaches the server for them.
  - **How a UI's URL reaches the server** (Blocked: D-171). The pages
    address the deployment by relative paths (`manifest`,
    `blobs/<hash>`, `edits`, `stops`), and the server answers no
    cross-origin request.
  - **Sign-in at each URL.** A page signs in with its own address as
    the redirect (`web/src/signin.svelte.ts`), so each UI's URL is
    registered with the agency's identity provider; the operator's
    guides say so.
  - **Rollback per UI** (Blocked: D-171): the previous bundle
    redeployed to that UI's URL, and where the retained bundles live.
  Pinned by an e2e spec that serves two UIs at two origins over one
  server, rolls one back, and keeps the other on its newer build.

The build side and retention are in place (docs/architecture/release-artifacts.md): the pinned toolchain, reproducible modules gated
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
- Two UIs at two origins over one server: one rolls back while the
  other stays on its newer build, and each signs in at its own URL.
- D-171 is decided and encoded.
- D-81 is decided and encoded, and `docs/architecture/release-artifacts.md`
  states the compatibility policy shared with document-versioning.
