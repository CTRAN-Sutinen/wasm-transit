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
- **Off-thread instantiation.** The resolver instantiates modules
  and registers documents in a Worker and posts answers back;
  modules are pure (no clocks, no randomness — hosts pass time in),
  so no shared state is needed. The thread boundary lands in the
  engine choke point ([host-discipline.md](host-discipline.md)),
  never per component. The skip-the-re-hash candidate stays the
  cost gate's. The runner (`tools/caps-run.mjs`) is process-per-
  session and is unaffected.

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
- The resolver registers off the main thread, and the cost gate's
  ratchet numbers are recorded on the Worker path (e2e).
- QAF-49 is deleted in the change that lands this.
