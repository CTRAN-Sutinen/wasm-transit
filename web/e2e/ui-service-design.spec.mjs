// The service-design UI (src/uis/service-design) — a generated UI over the same host layer and
// the same render components: service-design questions answered left
// to right on one screen (docs/ARCHITECTURE.md, web section).
//
//   1. A feed becomes an unframed plan; its service types, a type's
//      routes, a route's directions, and one direction's timetable
//      (cap-directory `timetable`) all answer.
//   2. The import is not changed in place: the timetable's changes are
//      disabled until the plan is forked.
//   3. The fork takes a timetable gesture (cap-author set_headway):
//      the timetable marks the added trip against the baseline (a
//      kept trip is never "gone" — QAF-64), the
//      comparison (cap-analysis service_type_compare) names the route,
//      and the warnings say the new trip has no block.
//   4. A stop dropped from the direction leaves its column
//      (remove_direction_stop).
//   5. Coverage answers against a population layer.
//   6. The fleet (cap-runcut `blocks`) answers only once its panel
//      is opened.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

test("service design answers a plan, a fork, and its change", async ({ page }) => {
  await page.goto("/service-design.html");
  await page.getByTestId("plan-bar").locator("input[type=file]").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));

  // WK, SUN, and HOL — every calendar the feed names is a service
  // type; HOL's only trip is single-stop (dropped), so it runs none.
  const types = page.getByTestId("type-table");
  await expect(types.locator("tbody tr")).toHaveCount(3, { timeout: 10_000 });
  await types.locator("tbody tr", { hasText: "WK" }).click();

  // The fleet answers only while its panel is open: folded, it names
  // no count; opened, cap-runcut answers the type's blocks.
  const fleet = page.getByTestId("fleet");
  await expect(fleet.locator("summary")).toHaveText("Fleet");
  await fleet.locator("summary").click();
  await expect(fleet.locator("summary")).toHaveText(/^Fleet — \d+ blocks$/);

  const routes = page.getByTestId("route-table");
  await routes.locator("tbody tr", { hasText: "10" }).first().click();
  await expect(page.getByTestId("direction-tabs")).toContainText("Downtown");
  const timetable = page.getByTestId("timetable");
  await expect(timetable.locator("tbody tr")).toHaveCount(2);
  const changes = page.getByTestId("timetable-change");
  await expect(changes).toContainText("Fork the import");
  await expect(page.getByTestId("change-headway").getByRole("button", { name: "Apply" })).toBeDisabled();

  await page.getByTestId("fork-name").fill("Spring");
  await page.getByTestId("fork-date").fill("2026-11-02");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("plan-health")).toContainText("Forked");
  await expect(page.getByTestId("plan-bar")).toContainText("governs 2026-11-02");

  // Every 20 min 08:00–09:00: 08:00 and 08:40 stay, 08:20 is new.
  const headway = page.getByTestId("change-headway");
  await headway.getByLabel("headway minutes").fill("20");
  await headway.getByLabel("headway from").fill("08:00");
  await headway.getByLabel("headway until").fill("09:00");
  await headway.getByRole("button", { name: "Apply" }).click();
  await expect(timetable.locator("tbody tr")).toHaveCount(3);
  await expect(timetable.locator("tbody tr.added")).toHaveCount(1);
  await expect(page.getByTestId("timetable-delta")).toContainText("1 added, 0 retimed, 0 gone");
  await expect(page.getByTestId("compare")).toContainText("2 → 3");
  await expect(page.getByTestId("plan-health")).toContainText("unassigned trips");
  await expect(page.getByTestId("type-vs")).toContainText("vs (unframed import)");

  // Main & 3rd splits the two variants; dropping it leaves its column.
  const columns = await timetable.locator("thead th").count();
  await timetable.locator("thead th button", { hasText: "Main & 3rd" }).click();
  await page.getByTestId("change-stops").getByRole("button", { name: "Drop it" }).click();
  await expect(timetable.locator("thead th")).toHaveCount(columns - 1);

  const coverage = page.getByTestId("coverage");
  await coverage.locator("input[type=file]").setInputFiles(path.join(fixtures, "population.geojson"));
  await expect(coverage.locator("tbody tr")).toHaveCount(2);
  await expect(page.getByTestId("plan-health")).toContainText("Coverage against");
});

// A plan saved before the 2026-10-27 rename (`schedules` / `schedule`)
// still reads: restore canonicalizes stored documents through
// cap-author, so its trips are trips again — the regression a real
// browser with older plans hit (every list read as headway bands).
test("a plan stored under the old field names restores in the current shape", async ({ page }) => {
  const doc = JSON.parse(fs.readFileSync(path.join(fixtures, "../../../testdata/network.json"), "utf8"));
  const legacy = { ...doc, schedules: {}, blocks: {} };
  delete legacy.pattern_trips;
  // The pre-trip shape: exact starts per schedule, block trips as
  // (schedule, start) — what the fixture's trips were written as.
  for (const [id, t] of Object.entries(doc.pattern_trips)) {
    const starts = t.trips.map((trip) => trip.departs.start_secs);
    legacy.schedules[id] = { pattern: t.pattern, service_type: t.service_type, schedule: { kind: "exact_times", starts } };
  }
  for (const [id, b] of Object.entries(doc.blocks)) {
    legacy.blocks[id] = {
      ...b,
      trips: b.trips.map((r) => {
        const [schedule, start] = r.trip.split("@");
        return { schedule, start_secs: Number(start) };
      }),
    };
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
  const timetable = page.getByTestId("timetable");
  await expect(timetable.locator("tbody tr")).toHaveCount(3);
  await expect(timetable.locator("tbody tr").first()).toContainText("08:00");
});
