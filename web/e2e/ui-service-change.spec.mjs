// The service-change UI (src/uis/service-change) — the loop of a
// service change on one screen (docs/architecture/generated-uis.md):
//
//   1. A feed becomes the base network; a plan started from it picks on
//      a date, and the day types of its period answer (analysis
//      `day_types`): Mon–Fri and Sun.
//   2. The network is drawn on the map (directory `route_lines`); the
//      frequency grid is the other way to look at it.
//   3. The worklist (analysis `network_findings`) has nothing to say
//      about the fixture until a change makes a long wait: moving the
//      first trip of route 10 two hours earlier leaves Main & 1st with
//      no bus for 140 minutes, and the worklist names it as new in
//      this plan.
//   4. A watched finding is re-read after every change: undo, and it
//      reads fixed.
//   5. The proposal reads across every day type (analysis
//      `plan_compare`); a hub opens in the drawer; back and forward
//      move through the selection.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

async function startPlan(page) {
  await page.goto("/service-change.html");
  await page.getByRole("tab", { name: "Plans" }).click();
  await page.getByTestId("feed-input").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  await expect(page.getByTestId("plan-select")).toHaveValue(/plan-/, { timeout: 10_000 });
  // The base is not changed in place: the worklist says to start a plan.
  await page.getByRole("tab", { name: "Worklist" }).click();
  await expect(page.getByTestId("base-note")).toBeVisible();
  await page.getByRole("tab", { name: "Plans" }).click();
  await page.getByTestId("fork-name").fill("Spring");
  await page.getByTestId("fork-date").fill("2026-11-02");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText('Started "Spring"');
}

test("the loop: a finding made, watched, and fixed", async ({ page }) => {
  await startPlan(page);

  // The plan's day types, in words.
  const days = page.getByTestId("day-types");
  await expect(days.getByRole("button", { name: "Mon–Fri" })).toHaveAttribute("aria-pressed", "true");
  await expect(days.getByRole("button", { name: "Sun" })).toBeVisible();

  // The network on the map; the grid is a click away.
  await expect(page.getByTestId("network-map")).toBeVisible();
  await page.getByRole("button", { name: "Frequency grid" }).click();
  await expect(page.getByTestId("frequency-grid")).toBeVisible();
  await page.getByRole("button", { name: "Map" }).click();

  // Nothing stands out in the fixture.
  const worklist = page.getByTestId("worklist");
  await expect(worklist).toContainText("Nothing stands out");

  // Open route 10 over the map and move its first trip to 6:00.
  await page.getByRole("combobox", { name: "Open a route" }).fill("10");
  await page.getByRole("listbox", { name: "Open a route" }).getByRole("option", { name: /^10 / }).click();
  const drawer = page.getByTestId("route-drawer");
  const line = drawer.getByTestId("string-line");
  await expect(line).toContainText("2 trips");
  await drawer.getByTestId("edit-toggle").click();
  await line.getByRole("button", { name: /Trip t10 at 8:00/ }).dispatchEvent("pointerdown", { clientX: 0, pointerId: 1 });
  await line.locator("svg").dispatchEvent("pointerup", { pointerId: 1 });
  const trip = drawer.getByTestId("gesture-trip");
  await expect(trip).toContainText("Trip t10, now 8:00");
  await trip.getByLabel("trip leaves at").fill("6:00");
  await trip.getByRole("button", { name: "Move" }).click();
  await expect(page.getByTestId("status")).toContainText("Changed.");

  // The worklist names the wait the change made.
  const wait = worklist.locator("li", { hasText: "Main & 1st" });
  await expect(wait).toContainText("140 min from 6:00");
  await expect(wait).toContainText("new in this plan");

  // Watch it: the drawer shows it beside the change.
  await wait.getByRole("button", { name: "Watch this finding" }).click();
  const watched = drawer.getByTestId("watch-list").getByTestId("watched");
  await expect(watched).toContainText("Main & 1st");
  await expect(watched).toContainText("140 min from 6:00 — new");

  // The proposal, across every day type.
  await page.getByRole("tab", { name: "Proposal" }).click();
  const score = page.getByTestId("plan-score");
  await expect(score).toContainText("Mon–Fri");
  await expect(score.getByRole("button", { name: "10" })).toBeVisible();

  // Undo, and the watched finding reads fixed.
  await page.getByRole("button", { name: "Undo" }).first().click();
  await expect(watched).toContainText("fixed");
  await page.getByRole("tab", { name: "Worklist" }).click();
  await expect(worklist).toContainText("Nothing stands out");
});

test("a hub opens in the drawer, and back closes it", async ({ page }) => {
  await startPlan(page);
  await page.getByRole("combobox", { name: "Open a stop or hub" }).fill("Main Station");
  await page.getByRole("listbox", { name: "Open a stop or hub" }).getByRole("option", { name: /^Main Station / }).first().click();
  const hub = page.getByTestId("hub-drawer");
  await expect(hub.getByTestId("stop-waits")).toContainText("2 departures, 8:15–8:35");
  await expect(hub.getByTestId("near-misses")).toBeVisible();
  // Back moves through the selection: the drawer closes.
  await page.goBack();
  await expect(hub).toHaveCount(0);
  await page.goForward();
  await expect(page.getByTestId("hub-drawer")).toBeVisible();
});
