# cap-plans

The plan-chain capability: which service plan governs a date? transit-plans is the library. The chain's ops are pure over the metadata the host pushes. A pick's checks also link transit-network and transit-stops, and its network and the stops document register on the kernel's loads (D-237).
- `resolve`, `periods`, and `validate` work over the plan set's metadata.
- `timeline` answers what a planning page lists (D-228): every plan on the device, then each published pick it does not hold; which are published (a published pick's id, or the deployment's feed); the pick in force; and the chain's periods and issues. A retired pick is no row.
- `publish`, `retire`, `in_force`, and `in_use` work over the published plan set. `publish`, `retire`, and `in_use` take the host's date.
- The host owns the plan set and pushes its metadata on each call.
- `publish` answers the next set's canonical bytes and hash, which the server stores as given.
- `freeze` and `adopt` answer the next frozen versions and the next set, each with its records for the plans log (`next`).
- `log_records` and `log_document` turn the published set and the frozen versions into the plans log's records (`pick`, `frozen_version`, and each document's own record) and back. cap-log keeps the log and never reads a record.
- `receive` writes the plans log whole (D-228): `freeze`, `adopt`, and `retire` over the documents at the head, which the host gives inline, then the entry. A publish is a freeze and an adoption in one call. `pick` is the server's whole write of a pick: transit-network's `publish_check` (refused as cap-network's `publishable` answers), the stop records on its first date when adopted (transit-stops `read_pick`), the freeze, and the adoption. Its answer's `made` is the network the host stores as the head of `plan.<id>` before the entry. `migrate` freezes each pick of a set published before the log, and records both documents (the log's first entry).
