// The stop display (src/uis/stop-display) — a generated UI for the
// screen at a stop (docs/ARCHITECTURE.md, web section, "Generated
// UIs"), in three full-screen layouts over one answer.
//
// Friday 2026-10-23 at 07:55 (UTC in the browser), the fixture feed,
// and its trip updates (sample-rt.pb: t10 +120 s at A — served for
// every live feed URL):
//
//   1. The setup picks a layout and a stop (directory `stops`); the
//      URL keeps both. Several routes at A (board `grouped_board`):
//      10 → Downtown leaves in 7 min, 2 min late, live, then 45 min;
//      20 → Downtown in 25 min, scheduled. The alerts slot says none.
//   2. A transit center opened from its URL: the station pin pools its
//      bays, each time naming the bay it leaves from.
//   3. A single route: a detour in the edit document takes the 10 from
//      C, and its group says so; after the day's last departure at A,
//      its groups stay and say service has stopped.
//
// The live feeds and the edit document are served by the test (the
// shared e2e server holds other specs' edits).

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

test.use({ timezoneId: "UTC" });

const detour = [
  {
    id: "e-stop-display",
    rev: 1,
    status: "accepted",
    updated_at: 1,
    diversion: {
      id: "d-stop-display",
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

async function open(page, { query = "", at = "2026-10-23T07:55:00Z", edits = [] } = {}) {
  await page.clock.setFixedTime(new Date(at));
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify(edits) }));
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
  await page.goto(`/stop-display.html${query}`);
  await page.getByLabel("Agency GTFS zip").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
}

/// One group on the display, by its route and its destination exactly.
const group = (page, route, destination) =>
  page
    .locator(`[data-testid="display-group"][data-route="${route}"]`)
    .filter({ has: page.getByTestId("display-dest").getByText(destination, { exact: true }) });

test("the stop display is set up and shows several routes", async ({ page }) => {
  await open(page);

  // 1. The setup: a layout, then the stop by name.
  await expect(page.getByTestId("display-setup")).toBeVisible({ timeout: 10_000 });
  await page.getByTestId("layout-routes").click();
  const input = page.getByLabel("Stop", { exact: true });
  await input.fill("Main & 1st");
  await expect(page.getByRole("option").first()).toContainText("Main & 1st");
  await input.press("Enter");

  await expect(page.getByTestId("display")).toHaveAttribute("data-layout", "routes");
  await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st");
  await expect(page).toHaveURL(/stop=A/);
  await expect(page).toHaveURL(/layout=routes/);
  await expect(page.getByTestId("display-live")).toContainText("Live");

  const ten = group(page, "10", "Downtown");
  await expect(ten.getByTestId("display-time")).toHaveCount(2);
  await expect(ten.getByTestId("display-time").first()).toContainText("7 min");
  await expect(ten.getByTestId("display-time").first()).toContainText("2 min late");
  await expect(ten.getByTestId("display-time").nth(1)).toContainText("45 min");
  const twenty = group(page, "20", "Downtown");
  await expect(twenty.getByTestId("display-time").first()).toContainText("25 min");
  await expect(twenty.getByTestId("display-time").first()).toContainText("Scheduled");
  await expect(page.getByTestId("display-alerts")).toContainText("No service alerts");
});

test("a transit center names the bay each bus leaves from", async ({ page }) => {
  await open(page, { query: "?stop=STN&layout=center" });

  await expect(page.getByTestId("display-stop")).toHaveText("Main Station", { timeout: 10_000 });
  await expect(page.getByTestId("display")).toHaveAttribute("data-layout", "center");
  const toBayA = group(page, "40", "Bay A");
  await expect(toBayA.getByTestId("display-time").first()).toContainText("40 min");
  await expect(toBayA.getByTestId("display-time").first()).toContainText("Main Station: Bay B");
  const toBayB = group(page, "40", "Bay B");
  await expect(toBayB.getByTestId("display-time").first()).toContainText("Main Station: Bay A");
});

test("a single route says when the edits detour it away", async ({ page }) => {
  await open(page, { query: "?stop=C&layout=route", edits: detour });
  await expect(page.getByTestId("display-stop")).toHaveText("Main & 3rd", { timeout: 10_000 });
  await expect(group(page, "10", "Downtown").getByTestId("display-none")).toHaveText("Detoured — not serving this stop");
});

test("a route that has stopped for the day stays on the display", async ({ page }) => {
  await open(page, { query: "?stop=A&layout=route", at: "2026-10-23T23:00:00Z" });
  await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 10_000 });
  await expect(group(page, "10", "Downtown").getByTestId("display-none")).toHaveText("No more service today");
  await expect(group(page, "20", "Downtown").getByTestId("display-none")).toHaveText("No more service today");
});
