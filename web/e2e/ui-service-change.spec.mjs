// The service-change UI (src/uis/service-change) — a service change
// in the stages the work takes (docs/architecture/generated-uis.md):
//
//   1. Plan & calendar: a feed becomes the base network; a plan started
//      from it picks on a date; its service keys read as authored and
//      change in place (an exception date: Thanksgiving off the weekday
//      key).
//   2. Scan: the frequency grid first, the map one click away.
//   3. Route: a row opens one route's day — the string-line, each trip's
//      running time and key — and a trip moved there reads as moved; a
//      stop the route serves is read beside it.
//   4. Vehicles & hours: the fixture's trips on no block are named, and
//      adopting the suggested blocks leaves none.
//   5. Proposal: every day type, and the route changed.
//   Back and forward move through the stages.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

async function startPlan(page) {
  await page.goto("/service-change.html");
  await page.getByRole("button", { name: /Plan & calendar/ }).click();
  await page.getByTestId("feed-input").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  await expect(page.getByTestId("plan-select")).toHaveValue(/plan-/, { timeout: 10_000 });
  // The base is read, not changed: its calendar is shown, the edits off.
  await expect(page.getByTestId("service-key")).toContainText("Start a plan");
  await page.getByTestId("fork-name").fill("Spring");
  await page.getByTestId("fork-date").fill("2026-11-02");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText('Started "Spring"');
}

test("the stages: calendar, scan, route, vehicles, proposal", async ({ page }) => {
  await startPlan(page);

  // Plan & calendar: the weekday key, changed in place.
  const keys = page.getByTestId("service-keys");
  await keys.getByRole("button", { name: /^WK/ }).click();
  const key = page.getByTestId("service-key");
  await expect(key.getByRole("button", { name: "mon" })).toHaveAttribute("aria-pressed", "true");
  await expect(key).toContainText("10");
  await key.getByLabel("exception date").fill("2026-11-26");
  await key.getByLabel("exception state").selectOption("removed");
  await key.getByRole("button", { name: "Set" }).click();
  await expect(page.getByTestId("status")).toContainText("Changed.");
  await expect(key.getByTestId("exceptions")).toContainText("2026-11-26");
  const days = page.getByTestId("day-types");
  await expect(days.getByRole("button", { name: "Mon–Fri" })).toHaveAttribute("aria-pressed", "true");

  // Scan: the grid first, the map one click away.
  await page.getByRole("button", { name: /Scan/ }).click();
  const grid = page.getByTestId("frequency-grid");
  await expect(grid).toBeVisible();
  await page.getByRole("button", { name: "Map" }).click();
  await expect(page.getByTestId("network-map")).toBeVisible();
  await page.getByRole("button", { name: "Frequency grid" }).click();

  // Route: a row opens its day; each trip names its key.
  await grid.locator("tbody tr", { hasText: "10" }).first().click();
  await expect(page.getByTestId("stage-route")).toBeVisible();
  const line = page.getByTestId("string-line");
  await expect(line).toContainText("2 trips");
  const trips = page.getByTestId("trip-list");
  await expect(trips.locator("tbody tr").first()).toContainText("WK");

  // Move the first trip to 7:55; the trip list says it moved.
  await page.getByTestId("edit-toggle").click();
  await line.getByRole("button", { name: /Trip t10 at 8:00/ }).dispatchEvent("pointerdown", { clientX: 0, pointerId: 1 });
  await line.locator("svg").dispatchEvent("pointerup", { pointerId: 1 });
  const trip = page.getByTestId("gesture-trip");
  await trip.getByLabel("trip leaves at").fill("7:55");
  await trip.getByRole("button", { name: "Move" }).click();
  await expect(page.getByTestId("status")).toContainText("Changed.");
  await expect(trips).toContainText("moved from 8:00");

  // A stop the route serves, read beside its day.
  const hub = page.getByTestId("route-hub");
  await hub.getByRole("combobox").fill("Main & 1st");
  await page.getByRole("listbox", { name: "Read a stop or hub" }).getByRole("option", { name: /^Main & 1st/ }).click();
  await expect(hub.getByTestId("stop-waits")).toContainText("Waits at Main & 1st");

  // Vehicles & hours: the fixture's trips on no block, then adopted.
  await page.getByRole("button", { name: /Vehicles & hours/ }).click();
  const fleet = page.getByTestId("fleet-panel");
  await expect(fleet.getByTestId("unblocked")).toContainText("on no block");
  await fleet.getByTestId("adopt-blocks").click();
  await expect(page.getByTestId("status")).toContainText("Changed.");
  await expect(fleet.getByTestId("all-blocked")).toBeVisible();

  // Proposal: every day type; route 10 changed on weekdays.
  await page.getByRole("button", { name: /Proposal/ }).click();
  const score = page.getByTestId("plan-score");
  await expect(score).toContainText("Mon–Fri");
  await expect(score.getByRole("button", { name: "10" })).toBeVisible();

  // Back returns to the stage before.
  await page.goBack();
  await expect(page.getByTestId("stage-resources")).toBeVisible();
  await page.goForward();
  await expect(page.getByTestId("stage-proposal")).toBeVisible();
});
