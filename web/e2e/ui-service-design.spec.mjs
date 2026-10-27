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
import fs from "node:fs";

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

// A plan saved before the 2026-10-27 rename (`schedules` / `schedule`)
// still reads: restore canonicalizes stored documents through
// cap-author, so the trip list shows its starts instead of falling
// through to the headway-band message (the regression a real browser
// with older plans hit).
test("a plan stored under the old field names restores in the current shape", async ({ page }) => {
  const doc = JSON.parse(fs.readFileSync(path.join(fixtures, "../../../testdata/network.json"), "utf8"));
  const legacy = { ...doc, schedules: {}, blocks: {} };
  delete legacy.pattern_trips;
  for (const [id, t] of Object.entries(doc.pattern_trips)) {
    legacy.schedules[id] = { pattern: t.pattern, service_type: t.service_type, schedule: t.departures };
  }
  for (const [id, b] of Object.entries(doc.blocks)) {
    legacy.blocks[id] = { ...b, trips: b.trips.map((t) => ({ schedule: t.pattern_trips, start_secs: t.start_secs })) };
  }

  await page.goto("/service-design.html");
  await page.evaluate(async (plans) => {
    await new Promise((resolve, reject) => {
      const open = indexedDB.open("keyval-store");
      open.onupgradeneeded = () => open.result.createObjectStore("keyval");
      open.onsuccess = () => {
        const tx = open.result.transaction("keyval", "readwrite");
        tx.objectStore("keyval").put(plans, "transit:plans");
        tx.oncomplete = resolve;
        tx.onerror = () => reject(tx.error);
      };
      open.onerror = () => reject(open.error);
    });
    localStorage.setItem("transit:active-plan", "legacy");
  }, { legacy });
  await page.reload();

  await page.getByTestId("route-table").locator("tbody tr").first().click();
  await page.getByTestId("route-card").locator("details[open] button").first().click();
  const trips = page.getByTestId("schedule-change");
  await expect(trips.locator(".start")).not.toHaveCount(0);
  await expect(trips).not.toContainText("Headway-band");
});
