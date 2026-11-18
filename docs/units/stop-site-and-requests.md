# Stops: the agency's other data, and the work order system

The paper forms the planning team fills for a stop move into the
stops app. The Service Stop Worksheet is the stop's **site survey**,
answered on a phone or tablet at the stop (D-158, D-160;
`docs/architecture/crates.md`, transit-stops, and its Why;
`docs/architecture/generated-uis.md`, `stops`). The Bus Stop Sign &
Shelter Installation Worksheet is a **stop request**: the planning team's submission, a record of its own that tracks the work order the agency's work order
system opens for it (D-159; `docs/architecture/crates.md`,
transit-requests, and its Why). Photos and the site drawing are
**attachments** in a bucket of their own, named by content hash
(D-162). The stop's lifecycle stays the stops document's (D-157;
`docs/architecture/crates.md`, transit-stops; the stops page in
`docs/architecture/generated-uis.md`); a request is work, not a fact
about the stop.

## Scope, in order

1. **What the agency's other data says, beside the survey.** The
   schedule's side is on the card (the routes and directions, a time
   point, the stops either side and the distance to each). Left: the
   speed limit and lanes, which the streets document does not carry
   yet (its ways hold a class, a name, and one-way — the street
   attributes are [streets-model.md](streets-model.md)'s), the city
   and county, which no document on the bus holds, and a draft service
   change's schedule. Where the survey or a request's intent says
   otherwise than these, both are shown as given and the difference
   flagged (D-161).
2. **The work order system's real adapter**, written from its OpenAPI
   spec (the user brings it, D-165) behind the server's adapter
   interface (`server/workorders.go`); the stub, the fallback with none
   configured, and the page's controls are built
   (`docs/architecture/adjacent-systems.md`).

## Done when

- A dogfood session on C-TRAN fills a survey and a request on a phone
  viewport, every control walked.
