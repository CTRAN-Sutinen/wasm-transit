# UI: running today's service through disruptions

The disruptions page answers what is disrupted today, whether an alert
explains it, and what dispatch has noted. Status, Alerts, Dispatch, an
opened disruption with its notes, the alert form, what riders see of
an alert, and its preview are built and walked on C-TRAN
(`src/uis/disruptions/`, `e2e/ui-disruptions.spec.mjs`;
`docs/architecture/generated-uis.md`). This unit holds what is left.

Vehicle assignments and trip coverage are not here. CAD/AVL holds
them (D-122, D-212). The page stays one workspace for dispatch,
communications, and customer service until it is clearly overloaded
(D-209).

## Questions left

4. Where does this detour go: which stops does it skip, and what
   path does it take on the streets?
5. What does the detour do to the trip's path, the boards, and the
   trip plans?
8. Have the others working today seen this change or this note?

## Detours (questions 4 and 5)

The page lists detours and draws the stops they skip, but has no
screen to draw one. That screen needs its own design pass before it
is built.

- The ops exist: divert `draft`, streets `snap` and `route`, divert
  `propose`; then runcut `trip_path`, board `stop_board`, and router
  `plan_compare` with `edits_hash` for what the detour does. Streets
  traced route 72's detour along NE 137th Ave on the county roads
  (2026-11-06).
- A diversion draft is not persisted yet. When this UI first keeps
  one, the draft gains its kind and version tags and a schema lock
  like every persisted kind (`docs/FORMATS.md`, "Versions" and "The
  schema lock"), and its exemption in cap-abi
  `tests/formats_inventory_test.rs` goes.
- A detour's alert starts from its gap, as a cancel's does now.

## Who has seen it (question 8)

No op answers it. It needs a decision first: whether the agency keeps
a record of who has read a change or a note, and where. sync
`sync_edits` and the notes log exist, and the manifest pins
`notes_hash`, so a page already sees a new note at its next read.

## The alert form

- "Applies to" takes stops, a route, a route at stops, a trip from a
  gap, and the whole agency. Left: a direction, a trip picked by hand,
  and a stretch of stops along a route.
- The window when service is affected (`impact`).
- "Created" and "Last edited", with who and when, from the alerts log.

## Other open items

- Dispatch's late list shows a bus twice when its block's next trip is
  also late (QAF-127).
- Dispatch's notes name their subject by id (QAF-128).
- Partial cancels (D-45).

## Done when

- Questions 4, 5, and 8 are answered on the page, or 8 is decided
  out of scope.
- The alert form takes every "Applies to" above, the window, and the
  log's "Created" and "Last edited".
- `e2e/ui-disruptions.spec.mjs` covers each, and a dogfood session on
  C-TRAN walks every control: a dispatcher draws a detour, writes its
  alert, and notes a cancelled trip; a communications writer writes an
  alert from a template; customer service reads all three without
  signing in.
