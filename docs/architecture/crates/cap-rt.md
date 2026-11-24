# cap-rt

The realtime capability.

**Decode.** `decode` (bulk-bytes op 0) reads a GTFS-rt protobuf
payload. An agency serves these bytes every few seconds. The answer is
the overview view: delays for each trip and stop, live vehicles, and
service alerts, all in the feed's ids. Each snapshot registers under
its content hash. The kernel's `load_rt` registers a snapshot and
answers only its hash.

**The schedule join.** cap-rt joins the feed to the network. It uses
the network's boundary, when the network was imported, for the
published `stop_sequence` numbering. The boundary is optional.

- `trip_progress` answers one trip's board: scheduled, delay, and
  predicted. A stop that the update skips carries the last stated
  delay. This is the GTFS-rt propagation rule.
- `late_trips` ranks late trips. It flags a trip that is delayed but
  dark. It takes `vp_hash` for agencies that serve the two feeds
  separately. C-TRAN does. Darkness is judged for each block. When a
  delay is propagated to a block's next trip while its bus runs the
  trip before, the vehicle is located. `position_trip_id` names the
  trip the vehicle is on. `vehicle_id` is beside the label. A vehicle
  with no label is named by its id.

**The live document.** `live` mints the live document (transit-live)
from the trip-updates, vehicle-positions, and service-alerts snapshots.
Any of the three may be present (`alerts_hash` names the alerts). Each
snapshot's alerts count once for each entity id. The answer also has
the summary a status panel reads: vehicles by status, and the late
list.

The live document registers where cap-rt mints it. When cap-rt is
linked beside the board and the router, they read it by `live_hash`.
It never crosses to the host and back. With `with_document: false`, the
answer leaves the document out too. The stop display uses this
(QAF-49). The browser's live views render the live document. No
capability joins rt again.

**The alerts feed.** GTFS-rt is also the serialization of the
agency's own service alerts. `encode_alerts` writes the service alerts
document (cap-alerts' kind) as an Alerts feed, and registers it as an
rt snapshot under the content hash of its bytes. So `live` reads the
agency's alerts by that hash, as a rider's app reads the feed.
`read_alerts` is the inverse: it reads an Alerts feed into the
document, so another app's alerts come across.
