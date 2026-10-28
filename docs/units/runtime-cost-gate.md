# Runtime cost gate

Every network consumer registers each document version itself
(kernel bulk op 256; `hashIn` in `web/src/uis/kit.ts` caches per
module per version). On C-TRAN a scratch Chromium bench measured
5.8–8.4 MB of wasm memory per consumer and ~300 ms per
registration at 4× CPU throttle — the direct parse plus the
canonical re-hash every module repeats for a document the import
already hashed, all on the main thread. Nothing gates these
numbers, so the next regression is invisible.

## Scope

- A ratchet spec in `web/e2e` over the fixture feed: linear memory
  per consumer after `load_network`, and ms per registration,
  against recorded ceilings that may only come down.
- The same measurement on C-TRAN as a dogfood record (not a gate —
  the feed is not committed).
- Candidates once it is measured, not before: skip the re-hash
  when the host passes the hash the import answered (the module
  stays the hash authority — it verifies, never trusts).
  Off-thread instantiation is [module-runtime.md](module-runtime.md)'s.
  cap-author keeps a buffered decoder beside the direct one
  (+77 KiB) because `apply` edits carry network sub-types.
  Splitting the network document: the whole GTFS spec lands in it
  (C-TRAN's is 9.6 MB, shapes and their distances included), so
  alignments, fares, demand-responsive service, and station
  internals are candidates for documents of their own. A split
  alignment document keys lines by alignment id and keeps them
  exact: C-TRAN's shapes carry 14–15-digit coordinates, and nothing
  rounds at import.

## Done when

- The ratchet spec runs in `just e2e` and fails on a raised
  ceiling.
- A C-TRAN measurement is recorded in the change that lands it.
