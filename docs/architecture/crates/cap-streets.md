# cap-streets

The street-graph capability answers where along the streets. Its ops:

- `import` (bulk-bytes op 0) reads a street layer into the streets
  document and registers it. It answers the document, which the host
  keeps, with its hash and size. The kernel's `load_streets` registers a
  document the host holds, under the same hash. It refuses a document
  that is not canonical.
- `snap` answers the nearest street position. This is the graph address
  that a trace's waypoint commits to.
- `route` answers the street geometry between two positions. It is the
  shortest path, starting after `from`, so trace legs chain. It also
  gives the length along the graph and the streets it follows. On
  parts of the graph that do not connect, it answers empty, and the
  caller falls back to a straight line. For the bus profile, the
  streets to confirm come back named. `confirm` is the op's parameter.
- `walks` answers the walk table between a registered network's stops,
  with its hash. The kernel's `load_walks` answers the same.
- `access` answers a point's walks to those stops, nearest first, each
  with its path. It also answers the direct walk to a second point. This
  is the router's place.

The walking graph is a derived entry, built on the first `walks`. The
street graph is built on the first `snap`. A host that only walks never
builds the street graph. The domain code is in the shared
transit-streets library. The module writes nothing.

Today the server uses cap-streets for the streets and the walk table it
publishes, and ride-now uses it for walks from a point to the stops
near it. Drawing a detour or a pattern on the streets is next
(`docs/units/ui-stops.md`, `docs/units/ui-disruptions.md`).
