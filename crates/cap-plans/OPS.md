# plans

The service-plan chain: which plan governs a date. Service changes are picks. There are a few picks a year, at defined dates. The chain over the plan set's metadata is derived and never stored. A pick's date is a target that moves while the pick is in development.

- `resolve` answers which pick is in force.
- `periods` gives a window for every member.
- `validate` reports violations of the chain's rules.

Unversioned plans are standalone. They never resolve, and they are never flagged. All ops are pure ops over the metadata that the host pushes on each call.

## resolve

Which plan is in force on the date: the latest pick at or before it. Ties are broken by id, so the answer is deterministic and does not depend on order.

Unversioned plans never resolve. When no pick has taken effect yet, the answer is null.

## validate

Chain validity: unique plan ids, every versioned plan effective-dated, and unique pick dates.

The op reports every violation, tagged by kind. Unversioned plans are standalone and are never flagged.

## periods

Every chain member's governing period, derived and sorted by pick date. A period is [effective_from, until the next pick).

The `until` end is computed and never stored.

## publish

Publish a pick: the op answers the published plan set with the plan (re)placed by id and naming its network by hash.

- The input is the published plan set (null for the first publish) and the plan.
- The picks are in (effective_from, id) order. One set of picks is one byte sequence, however it was published.
- A republish is the pick's next revision. Republishing a retired pick restores it.
- `today` is the host's date. A pick that is new, restored, or moved never lands before it.
- A pick republished under its own date always publishes. This is a correction to the pick in force.

When the plan can publish, the answer is the canonical document and its `document_hash`. The module is the hash authority. The sync server stores exactly these bytes under its plans pointer.

A plan cannot publish when it is unversioned, undated, or dated before today. It also cannot publish when it would put two picks on one day. In that case the answer lists every reason in `issues` and has no document.

## in_force

Which published pick governs the date, with its `network_hash`. Retired picks never govern.

A client uses this to choose the network to load from the published plan set that the manifest pins as `plans_hash`. The answer is null before the first pick takes effect.

A set whose `schema_version` this build does not speak is an error. The op never guesses.

## retire

Retire a pick that has not taken effect. Such a pick is dated after today, the host's date. The answer is the set with the pick marked retired.

The mark is the tombstone. The pick stays in the set and never governs. Republishing it restores it.

A pick that has taken effect is refused. To correct it, republish it. The op answers like `publish`.
