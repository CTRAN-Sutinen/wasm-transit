// End-to-end tests: real Chromium against the built app served by
// transit-server, exercising the flows a user actually performs.
//
// Fixture feed (sample-gtfs.zip):
//   t10: A 08:00 -> B 08:10 (shape sh1), t20: A 08:20 -> B 08:30
//   all Friday service. Fixture realtime (sample-rt.pb): t10 delayed
//   +120 s at A / +180 s at B, one vehicle at 45.44/-73.44 on t10.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

async function loadFeed(page) {
  // Deterministic clock: Friday 2026-10-23 at 07:30, so the fixture's
  // 08:00/08:20 trips are always upcoming regardless of real time.
  await page.evaluate(() => window.__setNow({ date: "2026-10-23", secs: 27000 }));
  await page.setInputFiles("input[type=file]", path.join(fixtures, "sample-gtfs.zip"));
  await expect(page.getByText(/Loaded|Restored/)).toBeVisible();
}

/// Wait until the MapLibre instance exists and its sources are live.
async function waitForMap(page) {
  await page.waitForFunction(() => {
    const map = window.__map;
    return map && map.getSource("stops") && map.getSource("itinerary");
  });
}

/// Source data, normalized: some sources hold a FeatureCollection, the
/// selected-trip source holds a bare Feature.
async function mapData(page, source) {
  return page.evaluate((name) => {
    const data = window.__map.getSource(name).serialize().data;
    return data.type === "FeatureCollection" ? data : { features: [data] };
  }, source);
}

test("loads cleanly with no console errors", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(`pageerror: ${e}`));
  page.on("console", (m) => {
    if (m.type() === "error" && !m.text().includes("favicon")) {
      errors.push(m.text());
    }
  });
  await page.goto("/");
  await expect(page.getByText(/wasm ready/i)).toBeVisible();
  await waitForMap(page);
  expect(errors).toEqual([]);
});

test("loading a feed populates summary, board and map", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  // summary card
  await expect(page.getByRole("cell", { name: "agencies" })).toBeVisible();

  // departures render for the first stop once selected
  await page.getByLabel("Stop").selectOption("A");
  const rows = page.locator("section:has(h2:text('Departures')) tbody tr");
  await expect(rows).toHaveCount(2, { timeout: 10_000 });

  // stops are drawn on the map
  await expect
    .poll(async () => (await mapData(page, "stops")).features.length)
    .toBe(2);
});

test("clicking a departure draws the trip path", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);
  await page.getByLabel("Stop").selectOption("A");
  const departures = page.locator("section", { has: page.getByRole("heading", { name: "Departures" }) });
  await departures.getByRole("row", { name: /08:00/ }).click();
  await expect
    .poll(async () => (await mapData(page, "trip")).features.length)
    .toBeGreaterThan(0);
});

test("trip planner draws the ridden segment, not the whole route", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  await page.getByLabel("From", { exact: true }).selectOption("A");
  await page.getByLabel("To", { exact: true }).selectOption("B");
  await page.locator("#plan-time").fill("07:30");
  await page.getByRole("button", { name: "Plan trip" }).click();

  // overlay shows the plan
  const overlay = page.locator(".plan-overlay");
  await expect(overlay).toBeVisible();
  await expect(overlay.getByText(/arrive/i)).toBeVisible();
  await expect(overlay.getByText("08:10", { exact: true })).toBeVisible();

  // the itinerary transit leg is the A->B segment of shape sh1:
  // 3 points (clipped), not the whole 5-point route
  await expect
    .poll(async () => {
      const data = await mapData(page, "itinerary");
      return data.features.map((f) => f.geometry.coordinates.length);
    })
    .toEqual([3]);
});

test("unplannable trip gives inline feedback", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  // B -> A has no service at any time (t10/t20 only run A -> B)
  await page.getByLabel("From", { exact: true }).selectOption("B");
  await page.getByLabel("To", { exact: true }).selectOption("A");
  await page.getByRole("button", { name: "Plan trip" }).click();

  await expect(page.locator(".plan-notice")).toContainText(/No trip found/i);
  const itinerary = await mapData(page, "itinerary");
  expect(itinerary.features.length).toBe(0);
});

test("refresh restores the feed from local cache", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  await page.reload();
  await expect(page.getByText(/Restored/)).toBeVisible({ timeout: 10_000 });
  await expect(page.getByRole("cell", { name: "agencies" })).toBeVisible();
});

test("realtime feed colors delays and vehicles", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  await page.setInputFiles("input[type=file]", path.join(fixtures, "sample-rt.pb"));

  // severity chip for the delayed trip (worst +180 s -> "late 1-5 min")
  await expect(page.getByText(/late 1–5 min/)).toBeVisible();

  // the vehicle dot carries the joined delay
  await expect
    .poll(async () => (await mapData(page, "vehicles")).features[0]?.properties.delay)
    .toBe(180);

  // the departure board predicts the delayed time: 08:00 + 120 s
  await page.getByLabel("Stop").selectOption("A");
  await expect(page.getByText("08:02")).toBeVisible();
  await expect(page.getByText("+2 min")).toBeVisible();
});
