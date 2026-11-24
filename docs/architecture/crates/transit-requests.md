# transit-requests

The stop requests document (D-159). It holds the planning team's requests for work at a stop. C-TRAN's Bus Stop Sign & Shelter Installation Worksheet is the source. It depends on no other model crate. cap-requests is the seam.

**A request** is a record by id. It has:
- the job (new, reinstall, relocate_remove, repair_improve, temporary);
- the stop;
- the status: drafted, submitted, ordered (the work order opened), completed, or cancelled (the tombstone);
- the worksheet's fields:
  - who asked and when;
  - the intended routes and direction (the routes are names the planner writes, not a reference into the network: a new stop can be for a route that no pick runs yet);
  - the street;
  - the cross street that the offset is measured from, the offset, and the side;
  - city, county, township, range, section, and parcel;
  - the marking;
  - the utility locate (the utilities as free text, the agency's own list);
  - the work order it tracks (system, id as the Control #, link, its status as last read, and when);
  - the completion;
  - a note;
  - its photos and drawing (`attachments`, by hash, D-162).

**Operations.**
- `checked` refuses only what makes a request wrong. Each refusal names its request.
- `put` places one request by id. `refuse_taken` is the refusal of a write that says a request is new and names a held id. `OPEN_STATUSES` are the statuses of work in flight.
- `work_order_call` is what to send the work order system for a request.
- `for_stop` gives a stop's requests.
- `list` gives the work across every stop, at the statuses and of the jobs asked.
- `counts` gives every request by status.

The document is canonical as it stands (requests by id). The crate keeps what it does not know. It is version 2 (D-234). The chain from version 1 lives here (`version`): every v1 document is a v2 document. `version::record_forward` brings a stored log record forward.

A stop's event names the request that it carries out (transit-stops `request`), and a request names its stop. The existence check judges both references ([FORMATS.md](../FORMATS.md), "References"). Both documents are kept in the stops log, so completing a request and adding its stop's event can be one entry (D-197).

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### A request for work is its own record, not a stop's event (transit-requests, cap-requests)

- **Holds:** a request for work at a stop is the installation worksheet that the planning team submits. It is a record of its own, in a document of its own, by id, and it changes by replacing it whole.
- **Holds:** a request tracks the work order that the work order system opens (system, id, link, status as last read). The app shows this as given and never decides it.
- **Holds:** a request's fields are the worksheet's. They are free where the worksheet's are free (the utilities, the work order system). The crate refuses only what makes a request wrong.
- **Holds:** a new stop's intended routes and direction are the request's, since no schedule serves the stop yet.
- **Holds:** a stop's event names the request it carries out. Nothing checks this across the two documents.
- **Because:** the sheet is not a work order and not a fact about the stop. It is work with its own life (drafted, submitted, ordered, completed, cancelled). The work order lives in the agency's work order system, which this app will one day call to open one and read its status.
- **Rejected:** events on the stop (it mixes the work into the stop's facts); a built-in list of utilities (one agency's list, not the format's).
- **Decided:** D-159.
