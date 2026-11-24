# The crates — the layering, bottom up

Each crate below has one file in `crates/`. The file holds the crate's
detail, and its Why: the choices that other code depends on. The
crate's `//!` comment holds the rest. A capability that has no file here
is described by its `OPS.md` (`node tools/caps-run.mjs tools <id>`).

## The libraries, bottom up

- [transit-network](crates/transit-network.md)
- [transit-router](crates/transit-router.md)
- [transit-codec](crates/transit-codec.md)
- [transit-analysis](crates/transit-analysis.md)
- [transit-dispatch](crates/transit-dispatch.md)
- [transit-rider](crates/transit-rider.md)
- [transit-rt](crates/transit-rt.md)
- [transit-live](crates/transit-live.md)
- [transit-version](crates/transit-version.md)
- [transit-sync](crates/transit-sync.md)
- [transit-log](crates/transit-log.md)
- [transit-plans](crates/transit-plans.md)
- [transit-runs](crates/transit-runs.md)
- [transit-stops](crates/transit-stops.md)
- [transit-requests](crates/transit-requests.md)
- [transit-notes](crates/transit-notes.md)
- [transit-disruptions](crates/transit-disruptions.md)
- [transit-alerts](crates/transit-alerts.md)
- [transit-streets](crates/transit-streets.md)

## The capabilities with a file

- [cap-alerts](crates/cap-alerts.md)
- [cap-analysis](crates/cap-analysis.md)
- [cap-author](crates/cap-author.md)
- [cap-authz](crates/cap-authz.md)
- [cap-board](crates/cap-board.md)
- [cap-calendar](crates/cap-calendar.md)
- [cap-directory](crates/cap-directory.md)
- [cap-disruptions](crates/cap-disruptions.md)
- [cap-divert](crates/cap-divert.md)
- [cap-gtfs](crates/cap-gtfs.md)
- [cap-log](crates/cap-log.md)
- [cap-migrate](crates/cap-migrate.md)
- [cap-network](crates/cap-network.md)
- [cap-notes](crates/cap-notes.md)
- [cap-plans](crates/cap-plans.md)
- [cap-requests](crates/cap-requests.md)
- [cap-router](crates/cap-router.md)
- [cap-rt](crates/cap-rt.md)
- [cap-runcut](crates/cap-runcut.md)
- [cap-stops](crates/cap-stops.md)
- [cap-streets](crates/cap-streets.md)
- [cap-sync](crates/cap-sync.md)

## Every module

Every module is the raw rustc output of an assembly crate with the same four flat exports. There is no wasm-bindgen. Do not reintroduce host glue.
