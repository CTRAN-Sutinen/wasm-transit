# cap-requests

The stop requests capability: what work has been asked for at a stop, and where is it? transit-requests is the library.
- `validate` gives what a write must pass, or the canonical document and its hash. This is the document that `load_stop_requests` answers.
- `requests_for` gives a stop's requests.
- `list` gives the work across every stop, by status and job, with every request counted by status.
- `put` places one request by id, checked. With no document, the first request begins one. With `new`, a held id is refused (`id_taken`).
- `work_order_call` answers what to send the work order system for a request: the request for an `open`, the work order it holds for a `refresh`. The host makes the call.
- `record_work_order` records what the work order system answered, on its request.
- `vocabulary` gives the lists that a request's answers come from, and which statuses are open work.
- `log_records` and `log_document` turn the document into the stops log's records (a `stop_request` record for each request, and the document's own record) and back (D-197). cap-stops' `receive` writes the stops log, the requests with it.
