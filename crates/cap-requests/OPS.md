# requests

The planning team's requests for work at a stop (library: transit-requests). The model is C-TRAN's Bus Stop Sign & Shelter Installation Worksheet. A request is one of these jobs: a new stop, a reinstall, a relocation or removal, a repair or improvement, or a temporary stop.

A request is not a work order. It is the agency's own record, kept by id. It tracks the work order that the agency's work order system opens for it. The work order tracking has these parts:

- the system;
- the work order id (the worksheet's Control #);
- a link;
- the status as last read, with the day it was read. The status is shown as given.

A request has this life: drafted, submitted, ordered (the work order is opened), and completed. Or the request is cancelled. Cancelled is the tombstone. Nothing is deleted.

A request concerns one stop, by the stops document's lasting id. For a new stop, the request holds the intended routes and direction, because no schedule serves the stop yet.

Every field but the stop, the job, and the status is optional:

- who asked, and when;
- the street;
- the cross street that the offset is measured from the center of;
- the offset in feet;
- the side of the street;
- city, county, township, range, section, and parcel;
- how the spot is marked;
- the utility locate: the ticket, the day called, who called, and the utilities called (free text, the agency's own list);
- the completion;
- a note.

A stop requests document registers on the kernel's `load_stop_requests`. The load is refused unless `validate` would pass the document.

## validate

What a write must pass. The op lists every refusal, and each refusal names its request and its problem.

The refusals are:

- an empty id (`no_id`);
- no stop (`no_stop`);
- a job, status, side, direction, or `marked_by` that this build does not know (`unknown`, naming the field and the value);
- a date that is not YYYY-MM-DD (`bad_date`: `requested_on`, `completed_on`, the locate's `called_on`, or the work order's `read_on`);
- an offset below zero (`negative_offset`);
- a request at ordered with no work order id (`no_work_order`);
- a request at completed with no `completed_on` (`no_completion`);
- a request completed before it was requested (`completed_before_requested`).

Nothing else is refused. A request's fields are the worksheet's, as the planner gives them.

When there is no refusal, the answer is the canonical document and its `document_hash`. The document holds requests by id, and it keeps every field that this repo does not know. The module is the hash authority. The hash is the same hash that `load_stop_requests` answers.

A document whose `schema_version` this build does not read is an error.

## requests_for

A stop's requests, in id order. Each request is given with its id. The document must be registered by `load_stop_requests`.

A stop with no requests answers none.

## list

The work across every stop: what is in flight, and where. The document must be registered by `load_stop_requests`.

The answer lists every request, each with its id, in id order. Two filters narrow the list:

- `statuses`: only the requests at these statuses.
- `jobs`: only the requests of these jobs.

Each filter applies only when given.

Beside the list, the answer holds counts. The counts are of every request in the document by status. The filters do not change them. They are in the statuses' order (drafted, submitted, ordered, completed, cancelled). They include only the statuses that some request has.

## put

One change: the document with `request` placed under `id`. A new request is added. A request already held under that id is replaced whole. Every other request stays as it was. With no `requests_hash`, the request is the first request of a new document.

The op then checks the result as `validate` checks a write. There are two outcomes:

- Every refusal is listed and there is no document.
- There is no refusal. The document is registered and answered with its `requests_hash`.

A page writes the answer through the deployment's receive path (`POST /stop-requests`), which checks it again.

## record_work_order

What the work order system answered, on request `id`. The server's adapter writes it.

The work order's reference replaces the one held. A field that the answer lacks (a link, the system) stays as it was.

A drafted or submitted request moves to ordered, and its work order is open. A request that is ordered, completed, or cancelled keeps its status.

The op then checks the result as a write is checked. There are two outcomes:

- Every refusal is listed and there is no document.
- There is no refusal. The document is registered and answered with its `requests_hash`.

A request that the document does not hold is an error.

## vocabulary

The lists that a request's answers come from. The op reads no document.

- jobs: `new`, `reinstall`, `relocate_remove`, `repair_improve`, `temporary`.
- statuses: `drafted`, `submitted`, `ordered`, `completed`, `cancelled`.
- sides: `north`, `south`, `east`, `west`.
- directions: `northbound`, `southbound`, `eastbound`, `westbound`, `clockwise`, `counterclockwise`.
- markings: `hub`, `curb_mark`, `stake`, `other`.
