# plans

The service-plan chain: which plan governs a date. Service changes are PICKS — a few a year at defined dates — and the chain over the plan set's metadata is DERIVED, never stored: a pick's date is a target that moves while the pick is in development. resolve answers which pick is in force; periods windows every member; validate reports chain invariant violations. Unversioned plans are standalone: they never resolve, never flag. Pure ops over the metadata the host pushes per call.

## resolve

Which plan is in force on the date: the latest pick at or before it, ties broken by id (deterministic, order-insensitive). Unversioned plans never resolve; no pick yet answers null.

## validate

Chain validity: unique plan ids, every versioned plan effective-dated, pick dates unique. Every violation is reported (kind-tagged); unversioned plans are standalone and never flagged.

## periods

Every chain member's governing period — [effective_from, until the next pick) — derived and sorted by pick date. The until end is computed, never stored.

## publish

Publish a pick: the published plan set (null for the first publish) with the plan (re)placed by id and naming its network by hash, picks in (effective_from, id) order — one set of picks is one byte sequence however it was published. A republish is the pick's next revision; republishing a retired pick restores it. today is the host's date: a pick that is new, restored, or moved never lands before it, while a pick republished under its own date (a correction to the pick in force) always publishes (D-76). Answers the canonical document and its document_hash (the module is the hash authority; the sync server stores exactly these bytes under its plans pointer), or — when the plan is unversioned, undated, dated before today, or would put two picks on one day — every reason in issues and no document.

## in_force

Which published pick governs the date — retired picks never do — with its network_hash — how a client chooses the network to load from the published plan set the manifest pins as plans_hash. Null before the first pick takes effect. A set whose schema_version this build does not speak is an error, never a guess.

## retire

Retire a pick that has not taken effect (dated after today, the host's date — D-75): the set with it marked retired, the tombstone — it stays in the set and never governs, and republishing it restores it. A pick that has taken effect is refused (correct it by republishing). Answers like publish.
