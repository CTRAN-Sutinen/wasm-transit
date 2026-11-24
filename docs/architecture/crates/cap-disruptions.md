# cap-disruptions

The disruptions capability: what is disrupted on a day, and does an alert explain it? transit-disruptions is the library. It reads the network, the edit document, the live document, the stops, the service alerts, and the notes, each by hash, and writes nothing.
- `today` gives the day by route, filtered to one route or one stop when asked.
- `gaps` gives what no alert explains.
- `start_alert` gives what a new alert starts from, for a gap or blank.
