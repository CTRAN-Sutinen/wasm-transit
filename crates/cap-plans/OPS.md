# plans

The service-plan chain: which plan governs a date. Service changes are picks. There are a few picks a year, at defined dates. The chain over the plan set's metadata is derived and never stored. A pick's date is a target that moves while the pick is in development.

- `resolve` answers which pick is in force.
- `periods` gives a window for every member.
- `validate` reports violations of the chain's rules.

Unversioned plans are standalone. They never resolve, and they are never flagged. The chain's ops are pure ops over the metadata that the host pushes on each call. A pick written to the plans log is checked here too (`receive`, its `pick` op): the network's publish check, and the stops it reads on its first date. Its network and the stops document register on the kernel's loads.

## resolve

Which plan is in force on the date: the latest pick at or before it. Ties are broken by id, so the answer is deterministic and does not depend on order.

Unversioned plans never resolve. When no pick has taken effect yet, the answer is null.

## validate

Chain validity: unique plan ids, every versioned plan effective-dated, and unique pick dates.

The op reports every violation, tagged by kind. Unversioned plans are standalone and are never flagged.

## timeline

What does a planning page list, and which pick is in force? `timeline` answers every plan on the device (`plans`, each with its key, its frame's id, name, and pick date, and `served` for the deployment's feed), then each published pick the device does not hold.

A device plan is `published` (in production, so read-only) when its id is a published pick's, or when it is `served`. A retired pick is not a row and publishes nothing. A pick the device does not hold has no `key` and is not `loaded`. `published` is the published plan set, if the deployment has one.

The answer also gives `in_force`, the published pick in force on `today` (null when no set, or before the first pick), and the `periods` and `issues` of the chain over every row that has an id. The host routes the rows and keeps its own keys.

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

When the plan can publish, the answer is the canonical document and its `document_hash`. The sync server freezes and adopts a pick in one entry of its plans log (`adopt`), and a browser's plan set publishes through it.

A plan cannot publish when it is unversioned, undated, or dated before today. It also cannot publish when it would put two picks on one day. In that case the answer lists every reason in `issues` and has no document.

## in_force

Which published pick governs the date, with its `network_hash`. Retired picks never govern.

A client uses this to choose the network to load from the published plan set that the manifest pins as `plans_hash`. The answer is null before the first pick takes effect.

A set whose `schema_version` this build does not speak is an error. The op never guesses.

## in_use

The picks in use on `today`, the host's date: the pick in force, then every published pick dated after today, in the set's order.

A pick that stopped governing is not in use. A retired pick is not in use either.

The sync server asks this at startup. It brings the networks of these picks forward to the version its modules read, and republishes each one under its own date (`docs/FORMATS.md`, "Versions"). The other picks stay in their version.

A set whose `schema_version` this build does not speak is an error.

## retire

Retire a pick that has not taken effect. Such a pick is dated after today, the host's date. The answer is the set with the pick marked retired.

The mark is the tombstone. The pick stays in the set and never governs. Republishing it restores it.

A pick that has taken effect is refused. To correct it, republish it. The op answers like `publish`.

## freeze

Freeze a version: write a network to the record by its hash, with what it is. The answer is the frozen versions with the network of `plan` by `network_hash`.

- The input is the frozen versions (null for the first freeze), the plan's frame, the network's hash, and `purpose`, in the agency's words ("proposal for public comment", "board packet").
- A freeze puts nothing in force. A frozen version is in the record and not in force.
- Freezing a network again replaces what it says it is. The plans log keeps what it said before.
- Each network of a plan is its own version.

The answer is the canonical document, its `document_hash`, and its records for the plans log (`next`). A blank `purpose`, an unversioned plan, or an undated plan is refused: the answer lists the reasons in `issues` and has no document.

## adopt

Adopt a frozen version: put it in force from its date. The answer is the published plan set with the version's pick placed under its own frame, as `publish` places it.

- Only a frozen version is adopted. A network that no frozen version has is refused (`not_frozen`).
- `stops_seq` is the stops log entry the adoption read. The pick names it, so its stops are the stops document at that entry.
- Every rule of `publish` holds: a pick never lands before `today`, and a chain stays valid.

The answer is like `publish`, with the set's records for the plans log (`next`). So every network that was ever in force is a frozen version.

## log_records

The plans log's records of a published plan set and of the frozen versions, as the log capability's `compose` takes them. Each document gives the records of its collection (`pick`, `frozen_version`) and its own record (`plans`, `frozen_versions`), which keeps the fields outside the collection.

The sync server asks this once, to seed the plans log from the plans pointer.

## receive

A write to the plans log, whole: the server's one call for `POST /plans`, `POST /plans/freeze`, `POST /plans/adopt`, `POST /plans/retire`, and the log's first entry. The host gives the plans log's documents at the head inline (`documents.plans`, `documents.frozen_versions`; absent: none). It registers a pick's network and the stops document on the kernel's loads first, and gives their hashes in `loaded` (`network_hash`, `stops_hash`). It takes the write (`write`: the fold's checkpoint, the entry's header, and the entries after the fold that the writer did not read) and the ops, each `{cap: "plans", op, args}`:

- `freeze` (`{plan, network_hash, purpose}`) and `adopt` (`{network_hash, today, stops_seq}`), as the ops of the same name take them, over the documents the earlier ops left. A publish is a freeze, then an adoption, in one entry.
- `retire` (`{id, today}`).
- `pick` (`{purpose?, from_network_hash?, adopt, today, stops_seq?}`): the network in `loaded.network_hash` written as a pick. A network that does not publish is refused with `{issues, refusals}`, as cap-network's `publishable` answers it. When `adopt`, the pick reads the stops document in `loaded.stops_hash`: each stop's record on its first date, and the warnings of the stops it visits that are not in service over its period. The pick is then frozen under `purpose` (absent: not frozen) and put in force when `adopt`. A migration that brings a published pick forward names the version it came from (`from_network_hash`), and the frozen version pins it (D-232). A plan issue is refused with `{issues}`. The answer's `made` is the network for the host to store as the head of `plan.<id>` before the entry, and its `answer` is `{plan_id, network_hash, timezone, warnings, stop_warnings}`. `POST /plans`, `POST /plans/freeze`, and `POST /plans/adopt` each send one `pick`.
- `migrate`: each pick of the published set frozen as "published before the plans log", and both documents recorded. The host sends it with the old `plans` pointer's head to give an empty log its first entry.

`today` is the agency's date, which the host's clock gives. The answer is the entry's canonical bytes, which the host stores at the header's seq, and `heads`, the hash of each document the write wrote. No entry comes back when the write changes nothing, when its key landed before (`landed`), when a record it changes changed in an unread entry (`overlap`), when an op is refused (`refused`, its index and its `issues` as `refusals`), or when an op does not run (`failed`).

## log_document

A document assembled from the plans log's records: `plans` from the `pick` records, or `frozen_versions` from the `frozen_version` records, with the document's own record. The answer is the canonical bytes and their hash. The picks are in (effective_from, id) order.

A record in another version is refused, and the refusal names both versions.
