// The stops UI (src/uis/stops, D-157): every stop's own life, apart from
// the service plans. Its own server (playwright.config.mjs, :8084) runs
// the durable store, which a stops write needs (POST /stops); no stops
// document is kept until the first change, which saves the stops of the
// network running today (cap-stops `seed`).
//
//   - a stop in service is read on a date, with the routes that stop
//     there in the schedule running that day;
//   - suspended for dates (a refused end before its start first), it
//     reads suspended on a day inside them, counted and listed so;
//   - it returns early, the return's date corrected, and it moves;
//   - a new stop is planned at a point on the map, permitted, and
//     retired — hidden from the map until retired stops are shown.

import { test, expect } from "@playwright/test";

test.use({ baseURL: "http://127.0.0.1:8084" });

test("a stop's life: suspended and back, moved, and a new stop planned, permitted, retired", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=B");
  const card = page.getByTestId("stop-card");
  await expect(card).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("stop-status")).toContainText("In service");
  await expect(page.getByTestId("stop-routes")).not.toContainText("No route");

  // A suspension whose end comes before its start is refused, in words.
  await page.getByTestId("action-suspended").click();
  await page.getByTestId("form-on").fill("2026-11-09");
  await page.getByTestId("form-until").fill("2026-11-05");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-refused")).toContainText("the end must come after the first day");

  // Suspended Monday 11-09 until Thursday 11-12, for paving.
  await page.getByTestId("action-suspended").click();
  await page.getByTestId("form-on").fill("2026-11-09");
  await page.getByTestId("form-until").fill("2026-11-12");
  await page.getByTestId("form-note").fill("Paving");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("status")).toContainText("Suspend: B from 2026-11-09 until 2026-11-12", { timeout: 15_000 });

  await page.getByTestId("stops-on").fill("2026-11-10");
  await expect(page.getByTestId("stop-status")).toContainText("Suspended since 2026-11-09, back 2026-11-12");
  await expect(page.getByTestId("stop-status")).toContainText("Paving");
  await expect(page.getByTestId("filter-suspended")).toContainText("1");
  await expect(page.getByTestId("map-key-suspended")).toBeVisible();

  // Back early on Wednesday — entered as Thursday, then corrected.
  await page.getByTestId("action-in_service").click();
  await page.getByTestId("form-on").fill("2026-11-10");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-status")).toContainText("In service since 2026-11-10", { timeout: 15_000 });
  const events = page.getByTestId("stop-event");
  const last = (await events.count()) - 1;
  await page.getByTestId(`fix-${last}`).click();
  await page.getByTestId("fix-on").fill("2026-11-11");
  await page.getByTestId("fix-save").click();
  await expect(page.getByTestId("status")).toContainText("Corrected in service on B", { timeout: 15_000 });
  await expect(events.last()).toContainText("2026-11-11");
  await expect(page.getByTestId("stop-status")).toContainText("Suspended", { timeout: 10_000 });

  // Moved, from a point clicked on the map.
  await page.getByTestId("stops-on").fill("2026-11-20");
  await expect(page.getByTestId("stop-status")).toContainText("In service");
  await page.getByTestId("action-move").click();
  await page.getByTestId("stop-life-map").locator("canvas").click({ position: { x: 300, y: 300 } });
  await expect(page.getByTestId("stop-form")).toContainText("New point:");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("status")).toContainText("Moved B from 2026-11-20", { timeout: 15_000 });
  await expect(events.last()).toContainText("Moved to");

  // A new stop, planned where the map is clicked, then permitted.
  await page.getByTestId("new-stop-start").click();
  await page.getByTestId("stop-life-map").locator("canvas").click({ position: { x: 340, y: 260 } });
  await page.getByTestId("new-id").fill("N1");
  await page.getByTestId("new-name").fill("Main & 9th");
  await page.getByTestId("new-save").click();
  await expect(page.getByTestId("status")).toContainText("Added stop N1", { timeout: 15_000 });
  await expect(page.getByTestId("stop-status")).toContainText("Planned since 2026-11-20");
  await expect(page.getByTestId("action-in_service")).toBeVisible();
  await expect(page.getByTestId("action-suspended")).toHaveCount(0);
  await page.getByTestId("action-permitted").click();
  await page.getByTestId("form-on").fill("2026-11-20");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-status")).toContainText("Permitted", { timeout: 15_000 });

  // Retired: the stop leaves the map and the list until retired stops
  // are shown.
  await page.getByTestId("action-retired").click();
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-status")).toContainText("Retired", { timeout: 15_000 });
  await expect(page.getByTestId("filter-retired")).toContainText("1");
  await page.getByRole("button", { name: "Close" }).click();
  const rows = page.getByTestId("stop-row");
  await expect(rows.filter({ hasText: "Main & 9th" })).toHaveCount(0);
  await page.getByTestId("filter-retired").click();
  await expect(rows.filter({ hasText: "Main & 9th" })).toHaveCount(1);
});
