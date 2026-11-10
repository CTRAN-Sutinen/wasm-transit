// The ride-now UI (src/uis/ride-now) — a generated UI for someone
// taking a trip today: a map with a panel over it
// (docs/architecture/generated-uis.md).
//
// Friday 2026-10-23 at 07:55 at the agency (the fixture's America/
// Montreal; the browser in UTC, which never decides), the fixture feed,
// the rider at Main Station, and its trip updates (sample-rt.pb: t10
// +120 s at A, +180 s at B — served for every live feed URL):
//
//   1. Near you answers on open (directory `stops_near` + a board per
//      place): the station first, its pooled bays' departures.
//   2. The type-ahead answers as you type (directory `stops`), a typo
//      included (D-48); a name no stop carries says how stops are
//      named; Enter picks.
//   3. The stop's board is realtime-first: t10 leaves 8:02, two
//      minutes late (board `stop_board` over rt `live`).
//   4. A saved stop answers on home (`watchlist_board`: the stop
//      board, realtime-first — t10 at 8:02), and the stop is among the
//      recent ones; it is still saved after a reload (the store's
//      watchlist document).
//   5. Directions: leaving now rides t10 at its prediction
//      (`plan_compare`) — leave in 7 min, 11 min door to door (D-40) —
//      the map draws the leg (`itinerary_paths`);
//      the last bus leaves 8:40 and arriving by 8:35 rides route 20
//      (`plan_arrive_by`).
//   6. A detour in the edit document: the skipped stop's board says
//      what no longer stops there; the trip to it carries the notice.
//      A cancel of t10 (D-45): the board lists its 8:00 as cancelled,
//      not as a departure (the router's law pins that plans never ride
//      it: cap-router `a_cancelled_trip_is_never_ridden`).
//   7. A capability that fails to load turns its frames off, and they
//      say so; the rest answers.
//   8. The network: the routes filter by long name (directory
//      `entities`); a route lists its directions' variants
//      (`route_detail`); a picked departure's bus is tracked stop by
//      stop (rt `trip_progress`, its stops named by `stops_by_ids`).
//
// The live feeds and the edit document are served by the test (the
// shared e2e server holds other specs' edits).

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

test.use({
  timezoneId: "UTC",
  geolocation: { latitude: 45.515, longitude: -73.515 },
  permissions: ["geolocation"],
});

const cancelT10 = [
  {
    id: "c-ride-now",
    rev: 1,
    status: "accepted",
    updated_at: 1,
    cancel: { id: "cancel-t10", trip_id: "t10", start_secs: null, date: "2026-10-23" },
  },
];

const detour = [
  {
    id: "e-ride-now",
    rev: 1,
    status: "accepted",
    updated_at: 1,
    diversion: {
      id: "d-ride-now",
      route_id: "10",
      direction_id: null,
      start_date: "2026-10-23",
      end_date: "2026-10-23",
      first_kept_stop: "A",
      last_kept_stop: "B",
      skipped_stops: ["C"],
      added_secs: 300,
      detour: null,
    },
  },
];

async function open(page, { edits = [], staleAsOf = null } = {}) {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  const headers = staleAsOf ? { "X-Store-Stale": "true", "X-Store-As-Of": String(staleAsOf) } : {};
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", headers, body: JSON.stringify({ kind: "edit_records", schema_version: 2, records: edits }) }));
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
  await page.goto("/ride-now.html");
  await page.getByLabel("Agency GTFS zip").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  await expect(page.getByLabel("stop name")).toBeVisible({ timeout: 10_000 });
}

/// Type into a stop field and pick the first answer with Enter.
async function pick(page, field, text, name) {
  const input = page.getByLabel(field, { exact: true });
  await input.fill(text);
  await expect(page.getByRole("option").first()).toContainText(name);
  await input.press("Enter");
}

test("ride now answers near me, a stop, a save, and a trip", async ({ page }) => {
  await open(page);

  // The live feeds the deployment names are read on open.
  await expect(page.getByTestId("live-feeds")).toContainText("Live");

  // 1. Near you: the station first, its bays pooled.
  const nearby = page.getByTestId("nearby");
  const first = nearby.getByTestId("stop-card").first();
  await expect(first).toContainText("Main Station");
  await expect(first).toContainText("8:15");

  // 2. The type-ahead, a typo it still answers (D-48), and a name no
  // stop carries.
  await page.getByLabel("stop name").fill("mian 2nd");
  await expect(page.getByRole("option").first()).toContainText("Main & 2nd");
  await page.getByLabel("stop name").fill("zebra crossing");
  await expect(page.getByRole("listbox")).toContainText("No stop is named “zebra crossing”");
  await pick(page, "stop name", "main 1s", "Main & 1st");

  // 3. The board, realtime-first.
  const board = page.getByTestId("rider-board");
  await expect(board.locator("h2")).toHaveText("Main & 1st");
  const row = board.getByTestId("board-rows").locator("li").first();
  await expect(row).toContainText("8:02");
  await expect(row).toContainText("2 min late");
  await expect(row.locator("[data-status]")).toHaveAttribute("data-status", "late");
  await expect(board).toContainText("Live predictions");

  // 4. Save it; home shows it with its departures, and as recent.
  await board.getByRole("button", { name: "Save this stop" }).click();
  await expect(board.getByRole("button", { name: "Remove from saved" })).toBeVisible();
  await board.getByRole("button", { name: "back" }).click();
  const saved = page.getByTestId("saved");
  await expect(saved).toContainText("Main & 1st");
  await expect(saved).toContainText("8:02");
  await expect(saved).toContainText("2 min late");
  const recent = page.getByTestId("recent").getByRole("button", { name: "Main & 1st" });
  await expect(recent).toBeVisible();

  // 5. Directions from the stop: leaving now rides t10 at 8:02.
  await recent.click();
  await board.getByRole("button", { name: "Directions from here" }).click();
  const trip = page.getByTestId("trip-plan");
  await expect(page.getByLabel("From", { exact: true })).toHaveValue("Main & 1st");
  await pick(page, "To", "main 2", "Main & 2nd");
  const it = trip.getByTestId("itinerary");
  await expect(it).toContainText("8:02");
  await expect(it).toContainText("2 min late");
  await expect(it).toContainText("Arrive Main & 2nd at 8:13 am");
  // Door to door (D-40): leave at the boarding, 11 minutes to 8:13,
  // and "leave in" by the board's wait rule from 07:55.
  await expect(trip.getByTestId("duration")).toHaveText("11 min");
  await expect(trip.getByTestId("leave")).toHaveText("Leave in 7 min");
  await expect(page.getByTestId("ride-map")).toHaveAttribute("data-legs", "1");

  await trip.getByRole("radio", { name: "Last bus" }).click();
  await expect(trip.getByTestId("last-bus")).toBeVisible();
  await expect(it).toContainText("8:40");

  await trip.getByRole("radio", { name: "Arrive by" }).click();
  await trip.locator("input[type=time]").fill("08:35");
  await expect(it).toContainText("Arrive Main & 2nd at 8:30 am");

  // One linked module (D-77): every capability the session asked —
  // the board, the directory, rt, the router — answers in ride-now's
  // assembly, and the network they all read is held once.
  const bus = (fn, ...args) => page.evaluate(([fn, args]) => window.__bus[fn](...args), [fn, args]);
  const caps = await bus("caps");
  expect(caps).toEqual(expect.arrayContaining(["board", "directory", "rt", "router"]));
  for (const cap of caps) expect(await bus("module", cap)).toBe("ui:ride-now");
  const networks = (await bus("held", "router")).held.filter((e) => e.kind === "network");
  expect(networks).toHaveLength(1);
});

test("ride now keeps saved stops across a reload", async ({ page }) => {
  await open(page);
  await pick(page, "stop name", "main 1s", "Main & 1st");
  const board = page.getByTestId("rider-board");
  await board.getByRole("button", { name: "Save this stop" }).click();
  await expect(board.getByRole("button", { name: "Remove from saved" })).toBeVisible();

  // The watchlist document is the store's: it survives the visit.
  await page.reload();
  await page.getByLabel("Agency GTFS zip").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  const saved = page.getByTestId("saved");
  await expect(saved).toContainText("Main & 1st");
  await expect(saved).toContainText("8:02");
});

test("ride now shows a detour on the board and on the trip", async ({ page }) => {
  await open(page, { edits: detour });

  await pick(page, "stop name", "main 3", "Main & 3rd");
  const board = page.getByTestId("rider-board");
  await expect(board.getByTestId("board-skipped")).toContainText("8:45");

  await board.getByRole("button", { name: "Directions to here" }).click();
  await pick(page, "From", "main 1", "Main & 1st");
  await expect(page.getByTestId("plan-notice")).toBeVisible();
});

test("ride now lists a cancelled trip as cancelled, never as a departure", async ({ page }) => {
  await open(page, { edits: cancelT10 });

  await pick(page, "stop name", "main 1s", "Main & 1st");
  const board = page.getByTestId("rider-board");
  await expect(board.getByTestId("board-cancelled")).toContainText("8:00");
  await expect(board.getByTestId("board-skipped")).toHaveCount(0);
  await expect(board.getByTestId("board-rows")).not.toContainText("8:02");
});

test("ride now degrades frame by frame when a capability fails to load", async ({ page }) => {
  // The page's linked assembly does not load (one module: its failure
  // is every capability's — D-77), so each capability falls back to
  // its own module, and two of those fail too.
  await page.route(/\/caps\/(ui-ride-now|board|router)-[0-9a-f]+\.wasm$/, (route) => route.abort());
  await open(page);

  // Search still answers; the board says it is unavailable.
  await pick(page, "stop name", "main 1", "Main & 1st");
  await expect(page.getByTestId("rider-board")).toContainText("Departures are unavailable right now");

  await page.getByTestId("rider-board").getByRole("button", { name: "Directions from here" }).click();
  await pick(page, "To", "main 2", "Main & 2nd");
  await expect(page.getByTestId("trip-plan")).toContainText("Trip planning is unavailable right now");
});

test("ride now explores the routes and tracks a bus", async ({ page }) => {
  await open(page);

  // The routes, filtered by the long name a list shows.
  await page.getByRole("button", { name: "Routes" }).click();
  const list = page.getByTestId("route-list");
  await expect(list.locator("li")).toHaveCount(3);
  await page.getByLabel("filter routes").fill("downtown");
  await expect(list.locator("li")).toHaveCount(1);
  await expect(list).toContainText("Downtown via Main");

  // Route 10 runs two different shapes (A→B, and A→C→B): two variants.
  await list.locator("li button").first().click();
  const route = page.getByTestId("route-view");
  await expect(route.getByTestId("route-variant")).toHaveCount(2);
  await expect(route.getByTestId("route-variant").first()).toContainText("Main & 1st → Main & 2nd");
  await expect(route.getByTestId("route-variant").nth(1)).toContainText("via Main & 3rd");

  // A place on the route opens its board; t10's bus is reporting.
  await route.getByRole("button", { name: "Main & 1st" }).first().click();
  const board = page.getByTestId("rider-board");
  await board.getByTestId("board-rows").locator("li button").first().click();
  await expect(board.getByTestId("track")).toContainText("Bus 4201");
  await board.getByRole("button", { name: "Track this bus ›" }).click();

  const bus = page.getByTestId("bus-view");
  // A bus is as late as its trip's worst stated delay (+180 s at B).
  await expect(bus).toContainText("3 min late");
  const stops = bus.getByTestId("bus-stops").locator("li");
  await expect(stops).toHaveCount(2);
  await expect(stops.first()).toContainText("Main & 1st");
  await expect(stops.first()).toContainText("8:02");
  await expect(stops.nth(1)).toContainText("Main & 2nd");
  await expect(stops.nth(1)).toContainText("8:13");
});

// D-119: the edit document the server serves from its cache while its
// store does not answer is labelled with its age; ride now says so.
test("ride now says how old detours are while the server's store is not answering", async ({ page }) => {
  await open(page, { staleAsOf: new Date("2026-10-23T07:40:00-04:00").getTime() });
  await expect(page.getByTestId("ride-stale")).toContainText("Detours as of 7:40");
});
