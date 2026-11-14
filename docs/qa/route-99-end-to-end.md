# Route 99 end to end — a standing QA test

One new route, built the way an agency would build it, carried through
the four main apps: **stops**, **service change**, **ride now**, and
**stop display**. The route is proposed route 99 in C-TRAN's 2045
plan concept: Washougal – Camas – Lacamas Lake – East Fourth Plain.

The test passes when every step below passes **in the UI**, with no
step done through the runner or by hand. Run it after any change to
these four apps, the server's publish path, or the stops document.
Each run updates "Last run" and "Findings" below; a fixed finding is
deleted with its pin (the e2e spec or gate that now holds it).

## Setup

- `just qa`, run capped:
  `systemd-run --user --unit=qa-loop -p MemoryMax=8G --working-directory=$PWD -E PATH="$PATH" -E HOME="$HOME" just qa`.
  The server runs on :8091 over the C-TRAN feed
  (`testdata/ctran-gtfs.zip`) with a memory store (`mem://`), so
  every run starts clean. The pages are on :5174.
- Drive the pages with `node tools/ui-shot.mjs <ui> --hash … --steps walk.mjs`.
  Start from an empty browser profile (`--fresh` on the first page),
  so no draft plan from an earlier run is on the device.
- Run on a weekday before 7:30 pm (the agency's time), so route 99
  still has trips today. All dates below assume today is the run date;
  shift them to match.
- Stop `qa-loop` when done.

## The route

| Stop | Name | Lat, lon | From Washougal |
|---|---|---|---|
| 9906 | Washougal Transit Center | 45.5800, -122.3480 | 0 min |
| 9905 | NE 4th Ave & Everett St (Camas) | 45.5867, -122.4040 | 9 min |
| 9904 | NW Lake Rd & Lacamas Lake | 45.6080, -122.4230 | 15 min |
| 9903 | NE Ingle Rd & NE Goodwin Rd | 45.6480, -122.4300 | 25 min |
| 9902 | Fourth Plain & NE 192nd Ave | 45.6727, -122.4690 | 33 min |
| 9901 | East Fourth Plain Transit Center | 45.6727, -122.5060 | 40 min |

Route `099` "Washougal/Camas/East Fourth Plain", two directions (to
East Fourth Plain TC, to Washougal TC). Every stop is a timepoint.
Weekdays (`1-WKDY`) every 30 minutes, 6:00–20:00. Saturdays and
Sundays (`2-SAT`, `3-SUN`) every 60 minutes, 7:00–19:00. Trips to
Washougal start 15 minutes after the hour.

## Steps

### 1. Stops — each stop through the planning process

For each of the six stops, in the stops app (`stops.html#on=<today>`):

1. **New stop.** Type the point (Location step), then ID, name, code,
   start **Planned**, effective date (today − 10 weeks), and a note
   naming route 99. Leave "Create installation request" checked.
   *Pass:* "Added stop N"; the request page opens with job **New stop**.
2. **Installation request.** Routes `99`, Requested on = the planned
   date, **Submit**. *Pass:* the track reads Submitted.
3. **Work order.** **Open work order** (the stub adapter). *Pass:*
   `STUB-<request>` shown, track at Work order open.
4. **Permitted**, then **Built** (Record change, each dated, each
   carrying the request). *Pass:* the status line and History show each
   stage.
5. **Complete** the request (completed on the built date). *Pass:* the
   track reads Completed.
6. **Update stop record → In service**, dated today. *Pass:* "In
   service since <today>", with four History rows, each after the first
   naming the request.

*Pass for the step:* all six stops are in service today. The Work view
lists six completed New stop requests.

### 2. Service change — build route 99 and publish it

In the service-change app (`service-change.html#stage=calendar`):

1. **Fork** the plan in force: "Route 99 — 2045 concept", picks on
   today. *Pass:* the fork is the open draft.
2. **Stops stage:** find "East Fourth Plain" and "9901". *Pass:* each
   of the six new stops is found and opens on its card.
3. **Build the route:** create route 099, its two directions, a
   pattern each way through the six stops, timing, and the trips above.
   *Pass:* the Route stage shows 099 with 57 weekday trips, span
   6:00–20:40, a 40-minute running time.
4. **Vehicles & hours:** adopt the suggested blocks on each day type
   (Mon–Fri, Sat, Sun). *Pass:* every trip of route 99 is on a block.
5. **Proposal:** compare with the plan in force. *Pass:* the
   scorecard shows route 099 added, readiness has 0 issues and no
   stop warnings (every stop is in service).
6. **Publish.** *Pass:* "Published … takes effect <today>", and
   `GET /feed?date=<today>` lists route 099 and stops 9901–9906.

### 3. Ride now — plan a trip on it

In ride now (`ride-now.html`):

1. Find "Washougal Transit" by typing. *Pass:* Washougal Transit
   Center #9906 is the first answer; its board lists route 099 to East
   Fourth Plain TC every 30 minutes.
2. **Directions from here** to "East Fourth Plain Transit Center",
   leaving now. *Pass:* one ride on route 099, 40 minutes, drawn on the
   map.
3. **Last bus.** *Pass:* the 8:00 pm trip.
4. **Routes → 099.** *Pass:* both directions, six stops each.

### 4. Stop display — the stop from the trip

Open `stop-display.html?stop=9906&layout=route`, then
`?stop=9901&layout=routes`, then `?stop=9906&layout=center`.
*Pass:* each shows the stop's name and the next three route 099
departures with minutes to each; at 9906 to East Fourth Plain TC, at
9901 to Washougal TC; no bay tag at a stop that is not a station.

## Last run — 2026-11-13, 4:20–4:40 pm

Fails. Network import `sha256:d4b9b780…` (feed `aa8dbc62…`).

| Step | Result |
|---|---|
| 1. Stops | **Pass.** All six stops planned → permitted → built → in service, each with its request submitted, its work order opened and completed. No page errors. |
| 2.1 Fork | Pass. |
| 2.2 New stops in the plan | **Fail** — none of the six is found or drawn (QAF-113). |
| 2.3 Build the route | **Fail** — no control creates a route, pattern, or trip (QAF-112). Done instead through the runner (`caps-run`: author `apply` with `upsert_stop`, `upsert_route`, `upsert_pattern`, `upsert_pattern_trips`, then network `export`); the zip was imported as a draft, renamed, and dated today in the UI. |
| 2.3 (read back) | Pass — the Route stage shows 099: 57 trips, 6:00–20:40, 40 min, both directions. |
| 2.4 Blocks | Partial — adopting on Mon–Fri blocked the weekday trips (3 blocks, peak 80); Sat and Sun were not adopted, so 4 warnings remained. |
| 2.5–2.6 Proposal, publish | Pass — 0 issues, no stop warnings, published for 2026-11-13; `/feed` lists 099 and 9901–9906. |
| 3.1 Board | Pass — 099 at 5:00 … 8:00 pm, Scheduled. |
| 3.2–3.3 Trip, last bus | **Fail** — "Couldn't plan this trip right now (walk table … walks network d4b9…, not 7d07…)". Every trip fails, not only route 99 (Van Mall TC → Fisher's Landing TC fails the same way) (QAF-114, QAF-115). |
| 3.4 Routes | Not reached. |
| 4. Stop display | **Pass**, with one blemish — 9906 shows 099 in 20 min, 50 min, 6:00 pm; 9901 shows 5 min, 35 min, 5:45 pm. The center layout tags every time "Washougal Transit Center" (QAF-117). |

Note from the harness: every rider page logs "Cannot read properties
of undefined (reading 'waiting')" under `ui-shot`, which blocks service
workers (`installApp` in `web/src/uis/kit.ts` gets no registration).
It does not stop any step.

## Findings

IDs continue `docs/QA-FINDINGS.md`'s sequence.

- **QAF-112 UI GAP** — The service-change UI cannot create a route, a
  pattern, or a trip. The edits exist in cap-author (`add_route`,
  `add_pattern`, `add_pattern_trips`, `add_trip`, `upsert_*`), but its
  host sends only calendar, span, running-time, retime, and
  insert/remove-stop edits. A new route can reach a plan only as an
  imported GTFS zip.
  Pin: step 2.3; `web/src/uis/service-change/host.svelte.ts` (the
  `gestures` and `calendar` edits).
- **QAF-113 MODEL GAP** — A stop the stops app creates never reaches
  a plan. The service-change Stops stage does not find or draw it, no
  UI adds a stop to the network, and a published pick's `read_records`
  only updates stops the network already holds. The stops document and
  the plan meet only over stops the feed already had.
  Pin: step 2.2 (search "9901" on the fork: no matches).
- **QAF-114 DEFECT** — The server's walk table is always derived over
  the `-feed` import's network (`server/streets.go`, the `walks` op on
  `feed.networkHash`), never over the pick in force. Once a published
  pick that changes the network takes effect, ride now's trip planner
  refuses the walk table, and **no trip can be planned** for any rider.
  Pin: step 3.2; also Van Mall TC → Fisher's Landing TC, leave now.
- **QAF-115 UI COPY** — Ride now shows the router's refusal to the
  rider word for word, hashes and all, overflowing the panel.
  Pin: step 3.2 screenshot.
- **QAF-116 CONTRACT GAP** — A stop event may carry a request it
  predates: Permitted on 2026-10-01 was accepted naming request
  9901-20261113, requested 2026-11-13. Completing that request before
  its requested date is refused, so the two checks disagree.
  Pin: step 1, a request left at today's date while its stage events
  are backdated.
- **QAF-117 UI** — The stop display's center layout, at a stop that is
  not a station, tags every departure with the stop's own name where a
  bay would go.
  Pin: step 4, `?stop=9906&layout=center`.
