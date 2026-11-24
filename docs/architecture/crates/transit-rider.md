# transit-rider

A rider's own documents. The watchlist and the device records are the rider's, not the agency's. Nothing here reads the network or the operator's day, and transit-dispatch does not depend on this crate. cap-board reads the watchlist, and cap-migrate carries its version chain.

- The watchlist: a rider's saved stops, tagged at v2 (`kind`, `schema_version`). A copy from before the tags names `"version": "vN"`. Its v2 reads as v2, and its v1 comes forward through the chain here (`migrate_to`), each note becoming a label and each dropped color reported. `Versioned::version_in` reads the old marker, so the kernel's refusal names it.
- The device records ride-now keeps in the browser's store: `recent_places` (the last places a rider looked at) and `live_feeds` (the rider's own GTFS-rt URLs). Each is tagged at v1.
