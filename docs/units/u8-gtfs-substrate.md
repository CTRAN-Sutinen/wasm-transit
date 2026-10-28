# U8 — the GTFS substrate retires

transit-core is the GTFS serialization spec wearing core's name.
The network is the core model, and GTFS exists to translate it, so
the spec's one home is the codec.

## Scope

- Parse, calendar, and the deterministic zip write
  (`crates/transit-core/src/{parse,calendar,write}.rs`) fold into
  transit-codec.
- cap-gtfs re-points at the codec: the feed document's reads and
  the spec library sit on the same side of the boundary.
- transit-rt's joins move onto the network + the import's trip-id
  map (the model-side join already exists in `live_day`).
- The dead feed-side paths die rather than move:
  `apply_diversions` (`diversion.rs`) and `report_all` have no
  caller outside transit-core's own tests, and the feed-side day
  answers (`next_departures`, walking-group pooling, `blocks` /
  `blocks_on`, `block_trips`, `active_services`) have no bus
  caller.
- Every crate that depends on transit-core today (`grep
  transit-core crates/*/Cargo.toml`) drops the dependency or takes
  transit-codec instead. Model-side capabilities take neither —
  the Law 1 gate in `just test` already holds them to that.
- The Law 1 arch gate in the justfile names transit-core; it
  names the codec alone afterwards.

## Done when

- `crates/transit-core` is gone and `just check` is green.
- The root AGENTS.md tree, `docs/ARCHITECTURE.md`'s layering, and
  `crates/AGENTS.md` (Law 1's boundary list) no longer name it.
- A C-TRAN `caps-run` session imports, exports, and decodes rt
  with the same hashes as before the move (the feed and network
  hashes in `docs/QA-FINDINGS.md`'s header).
