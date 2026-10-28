// The ride-now UI (src/uis/ride-now) — a generated UI for someone
// taking a trip today: a map with a panel over it
// (docs/ARCHITECTURE.md, web section, "Generated UIs").
//
// Friday 2026-10-23 at 07:55 (UTC in the browser), the fixture feed,
// the rider at Main Station, and its trip updates (sample-rt.pb: t10
// +120 s at A, +180 s at B — served for every live feed URL):
//
//   1. Near you answers on open (directory `stops_near` + a board per
//      place): the station first, its pooled bays' departures.
//   2. The type-ahead answers as you type (directory `stops`); a name
//      no stop carries says how stops are named (QAF-70); Enter picks.
//   3. The stop's board is realtime-first: t10 leaves 8:02, two
//      minutes late (board `stop_board` over rt `live`).
//   4. A saved stop answers on home (`watchlist_board`, the schedule —
//      QAF-79), and the stop is among the recent ones.
//   5. Directions: leaving now rides t10 at its prediction
//      (`plan_compare`), the map draws the leg (`itinerary_paths`);
//      the last bus leaves 8:40 and arriving by 8:35 rides route 20
//      (`plan_arrive_by`).
//   6. A detour in the edit document: the skipped stop's board says
//      what no longer stops there; the trip to it carries the notice.
//   7. A capability that fails to load turns its frames off, and they
//      say so; the rest answers.
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

async function open(page, { edits = [] } = {}) {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00Z"));
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify(edits) }));
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

  // 2. The type-ahead, and a name no stop carries.
  await page.getByLabel("stop name").fill("vancouver mall");
  await expect(page.getByRole("listbox")).toContainText("No stop is named “vancouver mall”");
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
  await expect(saved).toContainText("8:00");
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
  await expect(page.getByTestId("ride-map")).toHaveAttribute("data-legs", "1");

  await trip.getByRole("radio", { name: "Last bus" }).click();
  await expect(trip.getByTestId("last-bus")).toBeVisible();
  await expect(it).toContainText("8:40");

  await trip.getByRole("radio", { name: "Arrive by" }).click();
  await trip.locator("input[type=time]").fill("08:35");
  await expect(it).toContainText("Arrive Main & 2nd at 8:30 am");
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

test("ride now degrades frame by frame when a capability fails to load", async ({ page }) => {
  await page.route(/\/caps\/(board|router)-[0-9a-f]+\.wasm$/, (route) => route.abort());
  await open(page);

  // Search still answers; the board says it is unavailable.
  await pick(page, "stop name", "main 1", "Main & 1st");
  await expect(page.getByTestId("rider-board")).toContainText("Departures are unavailable right now");

  await page.getByTestId("rider-board").getByRole("button", { name: "Directions from here" }).click();
  await pick(page, "To", "main 2", "Main & 2nd");
  await expect(page.getByTestId("trip-plan")).toContainText("Trip planning is unavailable right now");
});
