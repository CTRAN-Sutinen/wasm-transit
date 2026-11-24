# cap-author

The authoring capability: what does this network become after these edits? transit-network's edit module is the library.
- `apply` takes typed edits, atomic. It answers the next document under its hash, registered for the next change, with its issues and warnings.
- With `work_rules_hash`, the op judges a block's waits under the work rules' recovery (D-9).
- `created` names the entities that a change made.
- `touched` names the entities that the change altered beyond that, such as the patterns that a redrawn alignment moves.
- `stamp_pick` stamps an imported feed as a pick (D-140, D-228): one `set_plan` edit under the host's id and name, from the feed's first service date (`transit_network::stamp_as_pick`). A document that is a pick already, or runs no service, comes back as it was.

Every host authors through it, the service-design UI included. Its host sends edits and adopts the answer. It never edits a document.

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### An alignment edit names its reach (author `draw_alignment`, `redraw: one|all`, `touched`)

- **Holds:** redrawing a pattern's line says whether it redraws that pattern alone (a new alignment) or every pattern on the shared alignment. The edit answers the patterns it touched.
- **Because:** several patterns may run one alignment. A fix meant for one variant must never move the others silently. A fix meant for all must not need one edit per pattern.
- **Rejected:** always redraw the shared alignment; always copy it under a new id.
- **Decided:** D-11.

### Timetable gestures act on one service type (author `apply` gestures, `GesturePanel`)

- **Holds:** a gesture changes the trips of the one service type it names. The UI warns when other types run with it. There is no authored "same as, except" relation.
- **Because:** the reads pool a combination, but its members are separate authored types. A gesture that acts on all of them silently would change trips the planner did not pick.
- **Rejected:** gestures acting on the whole family; a "same as, except" relation (a model change, QAF-44).
- **Decided:** D-8.

### A fork carries weekly service through its period (author `set_plan {through}`)

- **Holds:** a fork with a period end extends every weekly service type that still runs in the calendar's last week to that end. The calendar is read up to the period's end. Dated-only types gain nothing.
- **Holds:** then the fork's calendar is its period. Every range is cut to the pick date through the period end. A range or exception outside the period goes. Every service type stays, even one left with no date.
- **Because:** a fork picked after the import's calendar runs nothing until every type is extended (13 edits on C-TRAN). Holidays are next year's to add deliberately.
- **Because:** dates outside the period never run in the fork, because a pick governs from its date. Keeping them only clutters its calendar. A type with no date still names its trips.
- **Rejected:** a separate `extend_service_ranges` gesture; leaving it to the planner by hand.
- **Decided:** D-41 (where `through` comes from: D-104; which types: D-112; the period as the calendar: D-138).
