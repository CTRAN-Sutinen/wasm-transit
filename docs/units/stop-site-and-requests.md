# Stops: the site, requests, attachments, and GTFS wiring

The paper forms the planning team fills for a stop move into the
stops app. The Service Stop Worksheet is the stop's **site survey**,
answered on a phone or tablet at the stop (D-158, D-160;
`docs/architecture/crates.md`, transit-stops, and its Why;
`docs/architecture/generated-uis.md`, `stops`). The Bus Stop Sign &
Shelter Installation Worksheet becomes a **stop request**: the planning team's submission,
a record of its own that tracks the work order the agency's work
order system opens for it (D-159). Photos and the site drawing are
**attachments** in a bucket of their own, named by content hash
(D-162). The stop's lifecycle stays the stops document's (D-157;
`docs/architecture/crates.md`, transit-stops; the stops page in
`docs/architecture/generated-uis.md`); a request is work, not a fact
about the stop. The stops' GTFS import and export are built as ops and
wait for their callers (item 5).

## Scope, in order

1. **What the schedule says, beside it.** For a stop a schedule
   serves (the published one, or a draft service change): its routes
   and directions, whether it is a time point on them, the previous
   and next stop and the distance to each, and the speed limit and
   lanes from the streets. Where the survey or a request's intent
   says otherwise, both are shown as given and the difference flagged
   (D-161).
2. **Stop requests.** A document kind of its own (versioned, its
   schema and worked example in `docs/FORMATS.md`), written through a
   receive path of its own: the kind of job (new, reinstall,
   relocate/remove, repair/improve, temporary), the stop, the intended
   routes and direction (a new stop is in no schedule yet), the
   requester and date, parcel and township/range/section, the measured
   offset from the cross street and the side of the street, how the
   spot is marked, the utility locate ticket, the date called and the
   utilities called, the completion and by whom, and the work order
   reference (system, id — the sheet's Control # — link, status as
   last read and when). Its life: drafted, submitted, work order
   opened, completed. A stop's `built`, `moved`, or `retired` event
   names the request it came from. A capability (library first) and
   the stops page's request form, phone-ready like the survey, the
   survey's draft kept the same way.
3. **Attachments.** A bucket of their own, named in the deployment's
   settings; a file is stored and named by its content hash, with its
   type, file name, caption, and who took it and when, on a request or
   a stop's event (a survey's photos: approaching, cross street). The
   server receives and serves them (the link is
   the server's, never stored); `just store gc` keeps every file a
   record names. The page's forms take a photo straight from the
   camera.
4. **The work order system's API.** The server opens a work order for
   a submitted request and reads its status back onto the request, as
   an adjacent system's writes are (`docs/architecture/adjacent-systems.md`).
   Which system, and its API, are taken up when this item is scoped.

5. **GTFS in and out, wired.** The library and its ops are built
   (cap-stops `export_versions` and `import_events`; D-150, D-151,
   `docs/architecture/crates.md`). Left: their callers. An export —
   the planner pages' Export and the server's `GET /feed` — runs
   `export_versions` over the pick's period first, so a stop moved or
   renamed inside it is one stops.txt row per version. An import — the
   server's feed at boot, a feed a page loads — runs `import_events`
   on the feed's first service date and writes the answer through the
   receive path; the stops page lists the events it added and the stops
   it did not list, each one fixable (a `moved` turned into a
   correction of the earlier point, stops `correct_event`).

6. **The pages sign in.** With sign-in on, a page's write carries no
   token, so the server refuses it (`DEPLOYMENTS-SERVER.md`, "Turn on
   sign-in", "Not yet") — a survey saved from the stops page included,
   whose surveyor is the person signed in (D-163). The page gets and
   refreshes the user's token from the agency's sign-in provider
   (OpenID Connect; Entra at C-TRAN) and presents it on writes only,
   reads staying public (D-58). Every page that writes needs it; it
   lands here because the field surveys need it first.

## Deferred

- **The stops page without a server's store, or offline** (the user,
  2026-11-12, deferred; 2026-11-13: "in the future we will definitely
  want to build full offline support", after the field forms here,
  D-158): a copy of the stops document on the device that every edit
  lands in first; begun from the deployment's document, the network it
  serves, a GTFS zip loaded in the page, or a stops file opened; saved
  to a file; sent to the deployment when its store answers; the page
  installed to work offline; several stops surveyed with no signal and
  sent later. Open: when the deployment's document moved on while
  edits waited — the edits replayed one by one onto it, each checked,
  the ones that no longer fit listed (recommended), or one of the two
  documents chosen.
- **Sharing the stops with a GIS**: GeoJSON for a date, JSON-FG over a
  range.

## Done when

- The request document has its laws in its crate, dispatch tests in their capabilities, and runner gates; the
  request kind's schema and worked example are published.
- `web/e2e/ui-stops.spec.mjs` submits a request with
  a work order reference and a photo, and completes it into the stop's
  `built` event; a store contract test keeps a referenced attachment
  through `gc`.
- An e2e with sign-in on saves a survey from the stops page, recorded
  under the signed-in user's configured name claim.
- An e2e exports a pick over a stop moved inside its period (two rows,
  one code) and imports a feed whose changes the stops page lists and
  fixes.
- A dogfood session on C-TRAN fills a survey and a request on a phone
  viewport, every control walked.
