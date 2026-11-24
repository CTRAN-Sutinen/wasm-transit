# cap-migrate

The migration capability: what is this older document in the version this build reads?
- `migrate` brings a document forward through every step of its chain:
  - a network (v1, every untagged shape, to v2);
  - an edit document (v1, the bare array, to v2);
  - a watchlist (v1, its notes and colors, to v2; a copy that names
    `"version": "v2"` from before the tags is already v2);
  - a service alerts document (v1 to v2, unchanged but its version).
- The chains live in the owning libraries (transit-network, transit-sync, transit-dispatch, transit-alerts). They are kept forever (D-66). Every record id is kept and every loss is a note.
- `migrate` answers the new document's canonical bytes and hash.
- `version` answers whether a document needs migration.

Consumers read the current version only and refuse any other at the load (`cap_abi::speaks`). So no consumer carries an old chain.

The capability is hosted where an older document arrives: the Go server's receive and startup, and service design's restore (docs/FORMATS.md, "Versions").
