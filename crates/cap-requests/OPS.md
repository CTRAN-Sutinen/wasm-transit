# requests

The planning team's requests for work at a stop (transit-requests; D-159) — C-TRAN's Bus Stop Sign & Shelter Installation Worksheet: a new stop, a reinstall, a relocation or removal, a repair or improvement, a temporary stop. A request is not a work order: it is the agency's own record, a record by id, and it tracks the work order the agency's work order system opens for it (system, id — the worksheet's Control # — link, and the status as last read, with the day it was read, shown as given). Its life: drafted, submitted, ordered (the work order opened), completed — or cancelled, the tombstone; nothing is deleted. A request concerns one stop by the stops document's lasting id; a new stop's intended routes and direction are the request's, since no schedule serves it yet. Every field but the stop, the job, and the status is optional: who asked and when, the street, the cross street the offset is measured from the center of, the offset in feet, the side of the street, city, county, township, range, section, parcel, how the spot is marked, the utility locate (ticket, day called, by whom, the utilities called — free text, the agency's own list), the completion, and a note. A stop requests document registers on the kernel's load_stop_requests, refused unless validate would pass it.

## validate

What a write must pass: every refusal, each naming its request and its problem — an empty id (no_id); no stop (no_stop); a job, status, side, direction, or marked_by this build does not know (unknown, naming the field and the value); a date that is not YYYY-MM-DD (bad_date: requested_on, completed_on, the locate's called_on, the work order's read_on); an offset below zero (negative_offset); a request at ordered with no work order id (no_work_order); one at completed with no completed_on (no_completion); one completed before it was requested (completed_before_requested). Nothing else is refused: a request's fields are the worksheet's, as the planner gives them. With no refusal, answers the canonical document — requests by id, every field this repo does not know kept — and its document_hash (the module is the hash authority: the same hash load_stop_requests answers). A document whose schema_version this build does not read is an error.

## requests_for

A stop's requests — each its id and the request — in id order, over a document registered by load_stop_requests. A stop with none answers none.

## put

One change: the document with `request` placed under `id` — a new request, or the one held replaced whole, every other request as it was; with no requests_hash, the first request of a new document. Then checked as a write is (validate): every refusal and no document, or none and the document registered and answered with its requests_hash. A page writes the answer through the deployment's receive path (POST /stop-requests), which checks it again.

## record_work_order

What the work order system answered, on request `id` (D-165; the server's adapter writes it): the work order's reference replaces the one held — a field the answer lacks (a link, the system) kept — and a drafted or submitted request moves to ordered, its work order open; one ordered, completed, or cancelled keeps its status. Then checked as a write is: every refusal and no document, or none and the document registered and answered with its requests_hash. A request the document does not hold is an error.

## vocabulary

The lists a request's answers come from: jobs (new, reinstall, relocate_remove, repair_improve, temporary), statuses (drafted, submitted, ordered, completed, cancelled), sides (north, south, east, west), directions (northbound, southbound, eastbound, westbound, clockwise, counterclockwise), and markings (hub, curb_mark, stake, other). Reads no document.
