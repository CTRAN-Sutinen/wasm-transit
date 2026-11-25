// The service-change UI (src/uis/service-change;
// docs/architecture/generated-uis/service-change.md).

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

async function startPlan(page) {
  await page.goto("/service-change.html");
  await page.getByRole("button", { name: /Plan & calendar/ }).click();
  await page.getByTestId("feed-input").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  await expect(page.getByTestId("plan-select")).toHaveValue(/plan-/, { timeout: 10_000 });
  // A feed imports as a draft pick on its first service date (D-140).
  const imported = page.getByTestId("plan-list").getByTestId("pick-row").filter({ hasText: "sample-gtfs" });
  await expect(imported).toContainText("2026-01-01");
  await expect(imported).toContainText("draft");
  await page.getByTestId("fork-name").fill("Spring");
  await page.getByTestId("fork-date").fill("2026-11-02");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText('Started "Spring"');
  // Nothing is compared until the planner chooses a plan on the Proposal (D-141).
  await expect(page.locator("header.bar").getByTestId("base-select")).toHaveCount(0);
  await page.getByRole("button", { name: /Proposal/ }).click();
  await expect(page.getByTestId("base-select")).toHaveValue("");
  await expect(page.getByTestId("stage-proposal")).toContainText("Choose a plan above");
  await page.getByTestId("base-select").selectOption({ label: "sample-gtfs" });
  await expect(page.getByTestId("plan-score")).toBeVisible();
  await page.getByRole("button", { name: /Plan & calendar/ }).click();
}

test("the stages: calendar, scan, route, vehicles, proposal", async ({ page }) => {
  await startPlan(page);

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

  await page.getByRole("button", { name: /Scan/ }).click();
  const grid = page.getByTestId("frequency-grid");
  await expect(grid).toBeVisible();
  await page.getByRole("button", { name: "Map" }).click();
  await expect(page.getByTestId("network-map")).toBeVisible();
  await page.getByRole("button", { name: "Frequency grid" }).click();

  await grid.locator("tbody tr", { hasText: "10" }).first().click();
  await expect(page.getByTestId("stage-route")).toBeVisible();
  const line = page.getByTestId("string-line");
  await expect(line).toContainText("2 trips");
  const trips = page.getByTestId("trip-list");
  await expect(trips.locator("tbody tr").first()).toContainText("WK");
  // A fixed viewBox letterboxes the chart on a wide screen.
  const runTimes = page.getByTestId("run-times");
  const [plotBox, cardBox] = [await runTimes.locator("svg").boundingBox(), await runTimes.boundingBox()];
  expect(plotBox.width).toBeGreaterThan(cardBox.width - 40);

  await page.getByTestId("edit-toggle").click();
  await line.getByRole("button", { name: /Trip t10 at 8:00/ }).dispatchEvent("pointerdown", { clientX: 0, pointerId: 1 });
  await line.locator("svg").dispatchEvent("pointerup", { pointerId: 1 });
  const trip = page.getByTestId("gesture-trip");
  await trip.getByLabel("trip leaves at").fill("7:55");
  await trip.getByRole("button", { name: "Move" }).click();
  await expect(page.getByTestId("status")).toContainText("Changed.");
  await expect(trips).toContainText("moved from 8:00");
  await expect(page.getByTestId("change-ledger").getByRole("button", { name: "10" })).toBeVisible();

  const hub = page.getByTestId("route-hub");
  await hub.getByRole("combobox").fill("Main & 1st");
  await page.getByRole("listbox", { name: "Read a stop or hub" }).getByRole("option", { name: /^Main & 1st/ }).click();
  await expect(hub.getByTestId("stop-waits")).toContainText("Waits at Main & 1st");

  await page.getByRole("button", { name: /Vehicles & hours/ }).click();
  const fleet = page.getByTestId("fleet-panel");
  await expect(fleet.getByTestId("unblocked")).toContainText("on no block");
  await fleet.getByTestId("adopt-blocks").click();
  await expect(page.getByTestId("status")).toContainText("Changed.");
  await expect(fleet.getByTestId("all-blocked")).toBeVisible();

  await page.getByRole("button", { name: /Proposal/ }).click();
  const score = page.getByTestId("plan-score");
  await expect(score).toContainText("Mon–Fri");
  await expect(score.getByRole("button", { name: "10" })).toBeVisible();

  await page.goBack();
  await expect(page.getByTestId("stage-resources")).toBeVisible();
  await page.goForward();
  await expect(page.getByTestId("stage-proposal")).toBeVisible();
});

test("stops: a stop's card lists every pattern that visits it", async ({ page }) => {
  await startPlan(page);
  await page.getByRole("navigation", { name: "Stages" }).getByRole("button", { name: /Stops/ }).click();
  await expect(page.getByTestId("stage-stops")).toBeVisible();
  await expect(page.getByTestId("stop-map")).toBeVisible();
  await expect(page.getByTestId("stop-none")).toBeVisible();

  const find = page.getByTestId("stage-stops").getByRole("combobox");
  await find.fill("Main & 2nd");
  await page.getByRole("listbox", { name: "Find a stop" }).getByRole("option", { name: /^Main & 2nd/ }).click();
  const card = page.getByTestId("stop-record");
  await expect(card).toContainText("Main & 2nd");
  await expect(card).toContainText("45.51000, -73.51000");
  const visit = card.getByTestId("stop-visit").first();
  await expect(visit).toContainText("trips");
  await expect(card).toContainText("10");
  expect(page.url()).toContain("open=B");

  await visit.getByRole("button").click();
  await expect(page.getByTestId("stage-route")).toBeVisible();
  await expect(page.getByTestId("string-line")).toBeVisible();
  await page.goBack();
  await expect(page.getByTestId("stop-record")).toContainText("Main & 2nd");

  await find.fill("Main Station");
  await page.getByRole("listbox", { name: "Find a stop" }).getByRole("option", { name: /^Main Station(?!:)/ }).click();
  await expect(card).toContainText("Station");
  await expect(card.getByTestId("stop-unserved")).toContainText("No pattern visits this station itself. Its stops are served by");
});

test("a new route: its stops picked in order, its trips per key, created as one change", async ({ page }) => {
  await startPlan(page);
  await page.getByRole("navigation", { name: "Stages" }).getByRole("button", { name: /Route/ }).click();
  await page.getByTestId("new-route-start").click();
  await expect(page.getByTestId("new-route")).toBeVisible();
  await page.getByTestId("new-route-close").click();
  await expect(page.getByTestId("new-route-start")).toBeVisible();

  await page.getByTestId("new-route-start").click();
  await page.getByTestId("new-route-id").fill("10");
  await page.getByTestId("new-route-name").fill("77");
  await expect(page.getByTestId("new-route-taken")).toContainText("already has route 10");
  await expect(page.getByTestId("new-route-next")).toBeDisabled();
  await page.getByTestId("new-route-id").fill("77");
  await page.getByTestId("new-route-long-name").fill("Crosstown");
  await page.getByTestId("new-route-next").click();

  await page.getByTestId("new-route-way-0-name").fill("To Main & 3rd");
  await page.getByTestId("new-route-way-0-run").fill("12");
  const add = page.getByTestId("new-route-way-0-add").getByRole("combobox");
  for (const name of ["Main & 1st", "Main & 3rd", "Main & 2nd"]) {
    await add.fill(name);
    await page.getByRole("listbox", { name: "Add a stop" }).getByRole("option", { name: new RegExp(`^${name}`) }).first().click();
    await expect(add).toHaveValue("");
  }
  const out = page.getByTestId("new-route-way-0-stops");
  await out.getByRole("button", { name: "Move Main & 2nd up" }).click();
  await expect(out.locator("li")).toHaveText([/Main & 1st/, /Main & 2nd/, /Main & 3rd/]);
  await page.getByTestId("new-route-way-1-name").fill("To Main & 1st");
  await page.getByTestId("new-route-way-1-run").fill("12");
  await expect(page.getByTestId("new-route-way-1-stops").locator("li")).toHaveText([/Main & 3rd/, /Main & 2nd/, /Main & 1st/]);
  await page.getByTestId("new-route-next").click();

  for (const dir of [0, 1]) {
    const row = page.getByTestId(`new-route-trips-${dir}`).getByTestId("new-route-trip-row").first();
    await row.getByLabel("Service key").selectOption("WK");
    await row.getByLabel("First departure").fill("07:00");
    await row.getByLabel("Last departure").fill("09:00");
    await row.getByLabel("Minutes between trips").fill("30");
  }
  await page.getByTestId("new-route-create").click();
  await expect(page.getByTestId("status")).toContainText("Route 77 created", { timeout: 15_000 });
  const stage = page.getByTestId("stage-route");
  await expect(stage).toContainText("Crosstown");
  await expect(page.getByTestId("string-line")).toBeVisible();
  await expect(stage).toContainText(/Trips\s*10/i);
  expect(page.url()).toContain("route=77");
});

test("plans, keys, routes and blocks: each control lands somewhere", async ({ page }) => {
  await startPlan(page);
  const status = page.getByTestId("status");

  const plans = page.getByTestId("plan-list");
  await plans.locator("tr", { hasText: "Spring" }).getByTestId("rename-plan").click();
  await plans.getByLabel("New name for Spring").fill("Spring 2027");
  await plans.getByRole("button", { name: "Save" }).click();
  await expect(status).toContainText('Renamed to "Spring 2027"');
  await expect(page.getByTestId("plan-select").locator("option:checked")).toHaveText("Spring 2027");
  // A fork picks on and runs through its period.
  await plans.getByTestId("pick-row").filter({ hasText: "Spring 2027" }).getByTestId("date-plan").click();
  await plans.getByLabel("New pick date for Spring 2027").fill("2026-11-02");
  await plans.getByLabel("Runs through for Spring 2027").fill("2026-11-30");
  await plans.getByRole("button", { name: "Save" }).click();
  await expect(status).toContainText("runs through 2026-11-30");
  await page.getByTestId("service-keys").getByRole("button", { name: /^WK\s/ }).click();
  await expect(page.getByTestId("service-key").getByLabel("range start", { exact: true })).toHaveValue("2026-11-02");
  await expect(page.getByTestId("service-key").getByLabel("range end", { exact: true })).toHaveValue("2026-11-30");
  await page.locator("header.bar").getByRole("button", { name: "Undo" }).click();
  await expect(page.getByTestId("service-key").getByLabel("range end", { exact: true })).toHaveValue("2026-12-31");
  const keys = page.getByTestId("service-keys");
  const before = await keys.locator(".list li").count();
  await page.getByTestId("add-key").click();
  await expect(keys.locator(".list li")).toHaveCount(before + 1);
  const added = (await page.getByTestId("service-key").locator("h2").textContent()).trim();
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

  await page.evaluate(() => { location.hash = location.hash.replace(/stage=[a-z]+/, "stage=scan") + "&route=40"; });
  await expect(page.getByTestId("stage-scan")).toBeVisible();
  await page.locator("nav.stages").getByRole("button", { name: /Route/ }).first().click();
  const dirs = page.getByTestId("direction-toggle").getByRole("button");
  await expect(dirs.first()).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("string-line")).toContainText("trips");
  const count = await dirs.count();
  expect(count).toBeGreaterThan(1);
  const first = await page.getByTestId("string-line").locator("h2").first().textContent();
  await dirs.nth(1).click();
  await expect(dirs.nth(1)).toHaveAttribute("aria-pressed", "true");
  await expect(dirs.first()).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByTestId("string-line").locator("h2").first()).not.toHaveText(first);
  await dirs.first().click();

  // The fixture's trips start on no block, so adopt the suggestion first.
  await page.locator("nav.stages").getByRole("button", { name: /Vehicles & hours/ }).click();
  await page.getByTestId("fleet-panel").getByTestId("adopt-blocks").click();
  await expect(status).toContainText("Changed.");
  await page.getByTestId("blocks").locator(".brow").first().click();
  await expect(page.getByTestId("block-chain").locator("tbody tr").first()).toBeVisible();

  await page.locator("nav.stages").getByRole("button", { name: /Route/ }).first().click();
  const block = page.getByTestId("trip-list").locator("button.blk").first();
  const blockId = (await block.textContent()).trim();
  await block.click();
  await expect(page.getByTestId("stage-resources")).toBeVisible();
  await expect(page.getByTestId("blocks").locator(".brow.on")).toContainText(blockId);
  await expect(page.getByTestId("block-chain").locator("tbody tr").first()).toBeVisible();

  await page.locator("nav.stages").getByRole("button", { name: /Route/ }).first().click();
  await page.getByTestId("close-route").click();
  await expect(page.getByTestId("stage-route")).toContainText("No route open");
  expect(await page.evaluate(() => location.hash)).not.toContain("route=");

  await page.locator("nav.stages").getByRole("button", { name: /Proposal/ }).click();
  await page.getByTestId("plan-score").locator("article").first().getByRole("button").first().click();
  await expect(page.getByTestId("stage-scan")).toBeVisible();

  await page.locator("nav.stages").getByRole("button", { name: /Plan & calendar/ }).click();
  page.once("dialog", (d) => d.accept());
  await plans.locator("tr", { hasText: "Spring 2027" }).getByTestId("delete-plan").click();
  await expect(status).toContainText('Removed "Spring 2027"');
  await expect(plans).not.toContainText("Spring 2027");
});

// A published pick is read-only (D-139). The server on :8084 is shared with
// published-picks and deploy-updates, which run first. The test retires the pick.
test.describe("a published pick", () => {
  test.use({ baseURL: "http://127.0.0.1:8084" });

  test("is read-only: its changes go into a fork", async ({ page, request, browser }) => {
    await startPlan(page);
    const status = page.getByTestId("status");
    const id = (await page.evaluate(() => new URLSearchParams(location.hash.slice(1)).get("plan"))).replace(/^plan-/, "");
    try {
      await page.getByRole("button", { name: /Proposal/ }).click();
      await page.getByTestId("publish").click();
      await expect(page.getByTestId("readiness")).toContainText(/published/i, { timeout: 10_000 });
      await page.getByRole("button", { name: /Plan & calendar/ }).click();
      const row = page.getByTestId("plan-list").getByTestId("pick-row").filter({ hasText: "Spring" });
      await expect(row).toContainText("published");
      await expect(row.getByTestId("rename-plan")).toHaveCount(0);
      await expect(page.getByTestId("service-key")).toContainText("is published");
      await expect(page.getByTestId("service-key").getByRole("button", { name: "mon" })).toBeDisabled();
      await page.getByTestId("fork-name").fill("Spring revised");
      await page.getByTestId("fork-date").fill("2026-11-09");
      await page.getByRole("button", { name: "Fork" }).click();
      await expect(status).toContainText('Started "Spring revised"');
      await expect(page.getByTestId("service-key").getByRole("button", { name: "mon" })).toBeEnabled();

      // Another device, nothing stored: the pick is on its timeline,
      // fetched by hash when it opens, and read-only there too.
      const fresh = await browser.newContext({ baseURL: "http://127.0.0.1:8084", serviceWorkers: "block" });
      const other = await fresh.newPage();
      await other.goto("/service-change.html#stage=calendar");
      const pick = other.getByTestId("plan-list").getByTestId("pick-row").filter({ hasText: "Spring" }).first();
      await expect(pick).toContainText("published", { timeout: 15_000 });
      if (await pick.getByRole("button", { name: "Open" }).count()) await pick.getByRole("button", { name: "Open" }).click();
      await expect(pick).toContainText("open");
      await expect(other.getByTestId("service-key")).toContainText("is published");
      await fresh.close();
    } finally {
      await request.post("/plans/retire", { data: { id } });
    }
  });
});
