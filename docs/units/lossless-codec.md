# The lossless codec

GTFS → network → GTFS is lossless over the whole GTFS Schedule
spec: every file, every field, ids included. The law is
`crates/transit-codec/tests/spec_round_trip_test.rs` — one
proptest per spec file, against an oracle that reads both zips as
raw tables (never through the parser under test). Each failing law
is a file the codec loses; the set of failing laws is this unit's
to-do list.

The document fixpoint (`codec_prop_test.rs`) stays: it catches
loss on export, while this law also catches loss on import, which
the fixpoint cannot see.

## Scope

- **Where non-model data lives.** Law 1 keeps GTFS out of the
  network model. What the network does not express (fares v1/v2,
  pathways and levels, flex locations and booking rules,
  translations, attributions, agency ids, trip/service/shape ids)
  needs a home at the boundary — e.g. a residual document the
  import keeps beside the network, addressed by hash, read only by
  the export. Decide this before writing codec code; the test's
  `round_trip` helper is the one seam that changes with it.
- **Model-side gaps the law exposes**: route long vs short names;
  minted trip, service, and shape ids; stop sequences renumbered;
  `shape_dist_traveled` fabricated where the feed has none and
  dropped from shapes; calendar.txt rewritten as calendar_dates;
  rider-identical duplicate trips collapsed; frequency-based trips;
  flex trips (windows instead of times).
- **The parser**: transit-core reads a subset of the spec; the
  boundary's parser reads all of it (with
  [u8-gtfs-substrate.md](u8-gtfs-substrate.md), it lives in the
  codec).
- The geometry split (alignments as their own document, for
  registration cost) must keep shapes exact: C-TRAN's shapes carry
  14–15-digit coordinates, so no rounding at import.

## Done when

- Every law in `spec_round_trip_test.rs` passes, and `just test` is
  green with it.
- A C-TRAN `caps-run` import → export reproduces the source feed
  under the same comparison (all 15 files, including the six fare
  files and the shapes' `shape_dist_traveled`).
