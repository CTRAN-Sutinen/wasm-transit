// End-to-end tests: real Chromium against the built app served by
// transit-server, exercising the flows a user actually performs.
//
// Fixture feed (sample-gtfs.zip, see make_fixtures.py):
//   t10: A 08:00 -> B 08:10 (shape sh1), t20: A 08:20 -> B 08:30
//   t30: A 08:40 -> C 08:45 -> B 08:50 (the interior-stop trip)
//   t11: A 09:00 -> B 09:10, SUN service only
//   t12: B 07:30, HOL service added on Friday 2026-10-23
//   all Friday service except where noted. Fixture realtime
//   (sample-rt.pb): t10 delayed +120 s at A / +180 s at B, one
//   vehicle at 45.44/-73.44 on t10.

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
  await expect(rows).toHaveCount(3, { timeout: 10_000 });

  // stops are drawn on the map
  await expect
    .poll(async () => (await mapData(page, "stops")).features.length)
    .toBe(3);
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

/// Create a diversion through the editor UI: route 10, anchors
/// A -> B, interior stop C auto-skipped, `mins` minutes added.
async function makeDiversion(page, mins) {
  await page.getByRole("button", { name: "New diversion" }).click();
  await page.getByLabel("Route").selectOption("10");
  // Click the two anchors in the stop-sequence list (A, C, B).
  await page.locator(".stop-seq").getByRole("button", { name: "Main & 1st" }).click();
  await page.locator(".stop-seq").getByRole("button", { name: "Main & 2nd" }).click();
  await page.getByLabel("Added time (min)").fill(String(mins));
  await page.getByRole("button", { name: "Apply diversion" }).click();
  await expect(page.getByText(/Diversion applied: 3 trips/)).toBeVisible();
}

test("diversion reroutes the schedule, the board and the planner", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  // Route 10, anchors A -> B, skip C, +5 minutes.
  await makeDiversion(page, 5);

  // C loses its only service (t30 used to run A -> C -> B).
  await page.getByLabel("Stop").selectOption("C");
  const rows = page.locator("section:has(h2:text('Departures')) tbody tr");
  await expect(rows).toHaveCount(0);

  // B's board shifts: t10 08:15, t30 08:55 (t12/t20 untouched).
  await page.getByLabel("Stop").selectOption("B");
  await expect(page.getByText("08:15", { exact: true })).toBeVisible();
  await expect(page.getByText("08:55", { exact: true })).toBeVisible();
  await expect(page.getByText("08:10", { exact: true })).toHaveCount(0);

  // The planner rides the diverted schedule and shows the delta.
  await page.getByLabel("From", { exact: true }).selectOption("A");
  await page.getByLabel("To", { exact: true }).selectOption("B");
  await page.locator("#plan-time").fill("07:30");
  await page.getByRole("button", { name: "Plan trip" }).click();
  await expect(page.locator(".plan-overlay")).toBeVisible();
  await expect(page.locator(".plan-overlay").getByText("08:15", { exact: true })).toBeVisible();
  await expect(page.locator(".plan-compare")).toContainText(/without the active diversion: arrive 08:10\s+\(5 min later\)/i);

  // A -> C is unreachable while the diversion is active; the notice
  // says what the baseline looked like.
  await page.getByLabel("To", { exact: true }).selectOption("C");
  await page.getByRole("button", { name: "Plan trip" }).click();
  await expect(page.locator(".plan-notice")).toContainText(/without it you would arrive 08:45/);

  // Toggle the diversion off: C's service returns.
  await page.locator(".diversion-list input[type=checkbox]").first().uncheck();
  await page.getByLabel("Stop").selectOption("C");
  await expect(rows).toHaveCount(1);
  await expect(page.getByText("08:45", { exact: true })).toBeVisible();

  // The edit persists in IndexedDB next to the feed: after a reload
  // it is still listed (retired, as toggled — the tombstone survives).
  await page.evaluate(() => window.__flushEdits());
  await page.reload();
  await waitForMap(page);
  await expect(page.getByText(/Restored/)).toBeVisible({ timeout: 10_000 });
  await expect(page.locator(".diversion-list li.inactive")).toHaveCount(1);
});

test("diversion ripple shows as a tight block layover", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  // Block B1: t10 08:00 -> 08:10, then t20 at 08:20 (10 min layover).
  const blocks = page.locator("section", { has: page.getByRole("heading", { name: "Blocks" }) });
  await expect(blocks.getByText("10 min")).toBeVisible();
  await expect(blocks.getByText("tight")).toHaveCount(0);

  // Divert route 10 with 6 extra minutes: t10 now ends 08:16 and the
  // layover shrinks to 4 minutes — flagged.
  await makeDiversion(page, 6);
  await expect(blocks.getByText("tight")).toBeVisible();
  await expect(blocks.getByText("4 min")).toBeVisible();
});

test("diversion syncs to the server and back", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  // Push: create a diversion (a local draft), sync, and the server
  // document must hold it as submitted. Other tests in this run may
  // have left retired records on the server (every reload syncs), so
  // assert on *this* record, not on document shape.
  await makeDiversion(page, 5);
  await page.getByRole("button", { name: "Sync with server" }).click();
  await expect(page.getByText(/Synced with server/)).toBeVisible();
  await expect(page.locator(".diversion-list")).toContainText("submitted");

  const doc = await page.evaluate(() => fetch("/edits").then((r) => r.json()));
  const mine = doc.find(
    (r) => r.status === "submitted" && r.diversion.skipped_stops?.includes("C"),
  );
  expect(mine).toBeTruthy();
  expect(mine.diversion.added_secs).toBe(300);

  // Pull: a second client pushes a record of its own; our reload must
  // surface it through the bootstrap sync.
  await page.evaluate(async () => {
    const doc = await fetch("/edits").then((r) => r.json());
    const mine = doc.find(
      (r) => r.status === "submitted" && r.diversion.skipped_stops?.includes("C"),
    );
    const record = {
      ...mine,
      id: "e-injected",
      diversion: {
        ...mine.diversion,
        id: "d-injected",
        added_secs: 120,
        skipped_stops: [],
      },
    };
    const res = await fetch("/edits", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify([record]),
    });
    if (!res.ok) throw new Error(`inject failed: ${res.status}`);
  });

  await page.reload();
  await waitForMap(page);
  await expect(page.getByText(/Restored/)).toBeVisible({ timeout: 10_000 });
  // Bootstrap sync pulled the injected record (+2 min, no skips)
  // alongside our own (skips C, +5 min).
  await expect(page.locator(".diversion-list")).toContainText("skips Main & 3rd");
  await expect(page.locator(".diversion-list")).toContainText("+2 min");
});
