# transit-plans

The service-plan chain, and the published plan set. cap-plans is the seam. This crate is cross-document logic, the counterpart of transit-sync's cross-record merge.

**The chain.** It works over the plan set's metadata. It is derived, never stored, deterministic, and order-insensitive.
- `resolve`: which pick is in force.
- `periods`: each member's governing window.
- `period_in`: the window one pick governs in a published set. The pick is placed by its id, so its own date wins, and a retired pick bounds nothing. transit-stops reads it for a pick's reads.
- `validate`: the chain invariants.

**The published plan set** (store kind `plans`, `PublishedPlans`).
- It holds every published pick's metadata and network hash.
- It is canonical: (effective_from, id) order, one entry per id.
- `publish` gives the next set, or every reason the pick is refused. The reasons are the chain, or a new, restored, or moved pick dated before today (D-76).
- `retire` retires a pick that has not taken effect. The pick is kept as the tombstone (D-75).
- `in_force` gives the published pick, and so the network, that governs a date. Retired picks never govern.
- `in_use` gives the picks in use on a date: the pick in force, then every published pick dated after it. A pick that stopped governing, or a retired one, is not in use.
- A pick names the stops log entry its adoption read (`stops_seq`, optional).

**The frozen versions** (store kind `frozen_versions`, `FrozenVersions`; D-191).
- They hold every network version the agency froze, by network hash: the plan's id, name, and date, and what the version is (`purpose`). A version that a migration brought forward to a newer network version pins the version it came from (`from_network_hash`, D-232).
- `freeze` gives the next frozen versions. `freeze_from` names the source as well; a refreeze that names none keeps the source it has. It refuses a blank purpose and a plan that is not a dated pick. A freeze puts nothing in force.
- `adopt` puts a frozen version in force: it is `publish` under the version's own frame, naming `stops_seq`. A network that no frozen version has is refused (`NotFrozen`). So every network in a published set is a frozen version.
