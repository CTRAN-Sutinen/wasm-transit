# Module runtime — bounded registries, off-thread instantiation

Every module keeps every registered document for the life of the
instance: the registries are append-only HashMaps keyed by content
hash, and nothing ever leaves. Rt snapshots are unique per poll
(an agency's bytes change every few seconds), so a dispatcher's
session grows by the live document's size forever — QAF-49
measured 388 KB per snapshot on C-TRAN, re-minted every poll. The
router caches a RAPTOR graph per (network, edits, date) the same
way: bounded by the host's choices in principle, unbounded across
a long session's dates and edit generations in practice. And every
module instantiates and registers on the MAIN thread
(`WebAssembly.instantiate` in the resolver) — the same thread the
UI renders on, at the ~300 ms per registration the cost gate
measured at 4× throttle.

The cost gate ([runtime-cost-gate.md](runtime-cost-gate.md))
ratchets the numbers; this unit builds the mechanisms.

## Scope

- **Eviction is a kernel op.** The module owns its registry (it is
  the hash authority), so the host cannot reach in; the host owns
  the documents, so the module must not decide alone. A kernel op,
  numbered from 256 like OP_LOAD_NETWORK, takes a keep-set of
  hashes and answers what it dropped: the host names what survives
  a navigation, the module enforces. Evicted-then-asked is the
  honest unknown-hash error ("load first"), never a wrong answer.
- **Document-kind defaults.** Ephemeral kinds (rt snapshots, the
  live document) default keep-last-N per producing hash — a poll
  loop must plateau without the host steering. Durable kinds
  (networks, feeds, edits, layers) evict only when the host says.
  The rt registry is the pin case.
- **The router's graph cache** bounds by the same op (the keep-set
  covers the day keys, not only documents).
- **The registry is one shared type.** Every module hand-rolls the
  same `OnceLock<Mutex<HashMap<String, Arc<T>>>>` per document kind
  (cap-board has four, cap-rt three). One registry type in `cap-abi`
  carries the kind's default (keep-last-N or host-evicted) and
  answers the eviction op, so a module declares its registries and
  never re-implements eviction.
- **Off-thread: one bus worker per page.** The resolver
  instantiates every module a page assembles in ONE Worker and
  registers documents there; the main thread sends op requests and
  receives answers. One worker, not one per module: documents cross
  between modules by hash inside it, and a network handed to N
  workers would be copied N times. Modules are pure (no clocks, no
  randomness — hosts pass time in), so no shared state is needed.
  The worker fetches a published document's BYTES and hands them to
  each module's bulk op: today the page parses the 9.6 MB network
  with `resp.json()`, then `JSON.stringify`s it back to bytes for
  the modules — the page never needs the network object, only
  answers. The thread boundary lands in the engine choke point
  ([host-discipline.md](host-discipline.md)), never per component;
  op calls become async there. The runner (`tools/caps-run.mjs`) is
  process-per-session and is unaffected.
- **Reclaiming memory.** Wasm linear memory never shrinks: eviction
  lets a module reuse what it freed, but a peak stays allocated for
  the instance's life. A module whose memory passes a ceiling is
  dropped and re-instantiated in the worker, and the host
  re-registers its keep-set (modules are pure, so the new instance
  answers the same bytes). This is the bound a stop display open
  for weeks relies on.

## Done when

- The eviction op is documented in the manifest (every module
  answers it) and gated in caps-smoke + the runner, with unit
  tests in the owning crates: evicted-then-asked errors, the
  keep-last-N default plateaus, a keep-set entry never drops.
- An e2e spec polls rt on the fixture and asserts the module's
  registry size and linear memory plateau.
- A C-TRAN dogfood record: a dispatcher-length session (hours of
  polls) stays bounded (the record, not the gate — the feed is not
  committed).
- The resolver registers off the main thread, the page never parses
  a published document, and the cost gate's ratchet numbers are
  recorded on the Worker path (e2e).
- e2e: a module forced past its memory ceiling is re-instantiated
  and answers the same bytes for the same request.
- QAF-49 is deleted in the change that lands this.
