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

// The controls that manage the plan set and move between the stages:
// every one shows its answer where the planner lands.
test("plans, keys, routes and blocks: each control lands somewhere", async ({ page }) => {
  await startPlan(page);
  const status = page.getByTestId("status");

  // A plan renamed; a service key added, then removed.
  const plans = page.getByTestId("plan-list");
  await plans.locator("tr", { hasText: "Spring" }).getByTestId("rename-plan").click();
  await plans.locator(".rename input").fill("Spring 2027");
  await plans.getByRole("button", { name: "Save" }).click();
  await expect(status).toContainText('Renamed to "Spring 2027"');
  await expect(page.getByTestId("plan-select").locator("option:checked")).toHaveText("Spring 2027");
  const keys = page.getByTestId("service-keys");
  const before = await keys.locator(".list li").count();
  await page.getByTestId("add-key").click();
  await expect(keys.locator(".list li")).toHaveCount(before + 1);
  const added = (await page.getByTestId("service-key").locator("h2").textContent()).trim();
  // Its only range removed, a range is added with the dates given.
  const key = page.getByTestId("service-key");
  await key.getByRole("button", { name: "Remove range" }).click();
  await expect(key.getByLabel("range start", { exact: true })).toHaveCount(0);
  await expect(key.getByTestId("add-range").getByRole("button", { name: "Add a range" })).toBeDisabled();
  await key.getByLabel("new range start").fill("2027-01-10");
  await key.getByLabel("new range end").fill("2027-05-31");
  await key.getByTestId("add-range").getByRole("button", { name: "Add a range" }).click();
  await expect(key.getByLabel("range start", { exact: true })).toHaveValue("2027-01-10");
  await expect(key.getByLabel("range end", { exact: true })).toHaveValue("2027-05-31");
  page.once("dialog", (d) => d.accept());
  await page.getByTestId("remove-key").click();
  await expect(keys.locator(".list li")).toHaveCount(before);
  await expect(keys).not.toContainText(added);

  // A route chosen while another stage is open reads in full when its
  // stage opens: its directions and its day.
  await page.evaluate(() => { location.hash = location.hash.replace(/stage=[a-z]+/, "stage=scan") + "&route=10"; });
  await expect(page.getByTestId("stage-scan")).toBeVisible();
  await page.locator("nav.stages").getByRole("button", { name: /Route/ }).first().click();
  await expect(page.getByTestId("direction-tabs")).toBeVisible();
  await expect(page.getByTestId("string-line")).toContainText("trips");

  // Every running block is drawn; one opens to its trips. (The
  // fixture's trips start on no block: adopt the suggestion first.)
  await page.locator("nav.stages").getByRole("button", { name: /Vehicles & hours/ }).click();
  await page.getByTestId("fleet-panel").getByTestId("adopt-blocks").click();
  await expect(status).toContainText("Changed.");
  await page.getByTestId("blocks").locator(".brow").first().click();
  await expect(page.getByTestId("block-chain").locator("tbody tr").first()).toBeVisible();

  // A trip's block, named on the Route, opens there with its chain.
  await page.locator("nav.stages").getByRole("button", { name: /Route/ }).first().click();
  const block = page.getByTestId("trip-list").locator("button.blk").first();
  const blockId = (await block.textContent()).trim();
  await block.click();
  await expect(page.getByTestId("stage-resources")).toBeVisible();
  await expect(page.getByTestId("blocks").locator(".brow.on")).toContainText(blockId);
  await expect(page.getByTestId("block-chain").locator("tbody tr").first()).toBeVisible();

  // A route closed: the Route stage is empty, and the address says so.
  await page.locator("nav.stages").getByRole("button", { name: /Route/ }).first().click();
  await page.getByTestId("close-route").click();
  await expect(page.getByTestId("stage-route")).toContainText("No route open");
  expect(await page.evaluate(() => location.hash)).not.toContain("route=");

  // A day type's card opens that day type on the Scan.
  await page.locator("nav.stages").getByRole("button", { name: /Proposal/ }).click();
  await page.getByTestId("plan-score").locator("article").first().getByRole("button").first().click();
  await expect(page.getByTestId("stage-scan")).toBeVisible();

  // The plan deleted from the device; the base opens.
  await page.locator("nav.stages").getByRole("button", { name: /Plan & calendar/ }).click();
  page.once("dialog", (d) => d.accept());
  await plans.locator("tr", { hasText: "Spring 2027" }).getByTestId("delete-plan").click();
  await expect(status).toContainText('Removed "Spring 2027"');
  await expect(plans).not.toContainText("Spring 2027");
});
