# caps-run across sessions

Within a session, documents cross capabilities by reference
(`--cap`, `--as`, `$refs`). Across sessions nothing persists, so
every dogfood session re-imports the feed. And the import's
response prints the whole network document — 2.7 MB on C-TRAN —
into the agent's context, which makes every real-data session
expensive before it asks its first question.

## Scope

- **A shaping flag** for responses that carry a whole document:
  print the hash and a summary, keep the document addressable by
  `$ref` within the session.
- **A session document store** so a later session reuses an
  import by hash instead of re-running it — or the journal
  ([host-discipline.md](host-discipline.md)), if it lands first;
  do not build both.
- Deferred until an agent session needs it: the trip map's reverse
  question ("which derived trip is agency id 757797?") still needs
  the whole map (539.6 KiB on C-TRAN); the forward question is
  shaped (`network.agency_trip_ids`).

## Done when

- A runner gate in `just test` chains two sessions over one import
  without re-importing.
- A C-TRAN session's stdout for import + `service_types_overview`
  is a few KB, not megabytes.
