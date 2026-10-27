// The service-design UI (src/uis/service-design) — a generated UI over the same host layer and
// the same render components: service-design questions answered left
// to right on one screen (docs/ARCHITECTURE.md, web section).
//
//   1. A feed becomes an unframed plan; its service types, a type's
//      routes, a route's schedules, and where it runs all answer.
//   2. The import is not changed in place: a schedule offers no
//      gesture until the plan is forked.
//   3. The fork takes the change (cap-author add_start), and the
//      selected type reads against the baseline it came from — with
//      the change's warnings (the new trip has no block).
//   4. Coverage answers against a population layer.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

test("service design answers a plan, a fork, and its change", async ({ page }) => {
  await page.goto("/service-design.html");
  await page.getByTestId("plan-bar").locator("input[type=file]").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));

  const types = page.getByTestId("type-table");
  await expect(types.locator("tbody tr")).toHaveCount(2, { timeout: 10_000 });
  await types.locator("tbody tr", { hasText: "WK" }).click();

  const routes = page.getByTestId("route-table");
  await routes.locator("tbody tr", { hasText: "10" }).first().click();
  const card = page.getByTestId("route-card");
  await expect(card).toContainText("Route 10");
  await card.locator("details[open] button").first().click();

  const schedule = page.getByTestId("schedule-change");
  await expect(schedule).toContainText("Fork the import");
  await expect(schedule.getByRole("button", { name: "+ trip" })).toHaveCount(0);

  await page.getByTestId("fork-name").fill("Spring");
  await page.getByTestId("fork-date").fill("2026-11-02");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("plan-health")).toContainText("Forked");
  await expect(page.getByTestId("plan-bar")).toContainText("governs 2026-11-02");

  await card.locator("details[open] button").first().click();
  const before = await schedule.locator(".start").count();
  await schedule.getByRole("button", { name: "+ trip" }).click();
  await expect(schedule.locator(".start")).toHaveCount(before + 1);
  await expect(page.getByTestId("plan-health")).toContainText("unassigned trips");
  await expect(page.getByTestId("type-vs")).toContainText("vs (unframed import)");
  await expect(page.getByTestId("type-vs")).toContainText("→");

  const coverage = page.getByTestId("coverage");
  await coverage.locator("input[type=file]").setInputFiles(path.join(fixtures, "population.geojson"));
  await expect(coverage.locator("tbody tr")).toHaveCount(2);
  await expect(page.getByTestId("plan-health")).toContainText("Coverage against");
});
