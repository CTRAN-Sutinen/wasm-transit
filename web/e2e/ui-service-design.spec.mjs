// The service-design UI (src/uis/service-design) — six views over one
// selection, chained (docs/ARCHITECTURE.md, web section; the unit
// docs/units/ui-service-design.md):
//
//   1. A feed becomes an unframed plan; its service groups answer, and
//      the Network view's frequency grid (analysis `route_frequency`)
//      lists the group's routes; a row opens the Route view, whose
//      string-line draws the direction's trips.
//   2. The import is not changed in place: the gestures are off until
//      the plan is forked; the frequency target says what it waits on.
//   3. The fork exports (the network's publish path), and one trip
//      moves on its own (retime_trip): the string-line marks it moved,
//      the change ledger names the route, and undo / redo put the
//      document versions back.
//   4. The selection lives in the URL hash: a reload returns to the
//      same view, route, and group.
//   5. Hubs answer a station's connections and corridors; coverage
//      answers the selected group (a combination of service types)
//      against a weighted layer (points or polygons).
//   6. A capability that fails to load fails its frames only, each
//      saying so, while the plan set keeps working.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

async function importFixture(page) {
  await page.goto("/service-design.html");
  await page.getByTestId("design-rail").getByRole("button", { name: "Plans" }).click();
  await page.getByTestId("feed-input").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  // The groups are the combinations that run together (D-55): WK
  // alone and SUN alone — HOL runs no trip, so it is in none.
  await expect(page.getByTestId("group-table").locator("tbody tr")).toHaveCount(2, { timeout: 10_000 });
}

test("service design chains its views over a plan, a fork, and a change", async ({ page }) => {
  await importFixture(page);
  const wk = page.getByTestId("group-table").locator("tbody tr", { hasText: "WK" });
  // A group is shown with the dates it holds on, never a day standing in for it.
  await expect(wk.getByTestId("group-dates")).toContainText("2026-01-01 – 2026-12-31");
  await wk.click();

  // Network: the group's ledger, the day's shape, and the grid.
  await page.keyboard.press("2");
  await expect(page.getByTestId("group-ledger")).toContainText("Trips");
  await expect(page.getByTestId("day-profile")).toBeVisible();
  const grid = page.getByTestId("frequency-grid");
  await expect(grid.locator("tbody tr")).toHaveCount(3);

  // A row opens the route: its string-line draws the direction.
  await grid.locator("tbody tr", { hasText: "10" }).first().click();
  await expect(page.getByTestId("view-route")).toBeVisible();
  await expect(page.getByTestId("string-line")).toContainText("2 trips");
  await expect(page.getByTestId("run-times")).toBeVisible();
  await expect(page.getByTestId("hour-gaps")).toBeVisible();

  // The import is the base: editing says to fork it; the frequency
  // target names what it waits on.
  await page.getByTestId("edit-toggle").click();
  const gestures = page.getByTestId("gestures");
  await expect(gestures).toContainText("Fork the import");
  await expect(gestures).toContainText("D-54");
  // A gesture changes one service type of the group (D-8); WK runs
  // alone, so nothing else is named.
  await expect(page.getByTestId("gesture-scope")).toContainText("Changes the trips of");
  await expect(page.getByTestId("sibling-warning")).toHaveCount(0);

  // Fork it from the Plans view.
  await page.keyboard.press("1");
  await page.getByTestId("fork-name").fill("Spring");
  await page.getByTestId("fork-date").fill("2026-11-02");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText("Forked");
  await expect(page.getByTestId("pick-timeline")).toContainText("Spring");
  await expect(page.getByTestId("plan-select")).toHaveValue(/plan-/);

  // The fork exports as a GTFS zip.
  await page.keyboard.press("6");
  const download = page.waitForEvent("download");
  await page.getByTestId("export").click();
  expect((await download).suggestedFilename()).toBe("Spring.zip");

  // One trip moves alone: pick its line, then move it to 07:55.
  await page.keyboard.press("5");
  const line = page.getByTestId("string-line");
  await line.getByRole("button", { name: /Trip t10 at 8:00/ }).dispatchEvent("pointerdown", { clientX: 0, pointerId: 1 });
  await line.locator("svg").dispatchEvent("pointerup", { pointerId: 1 });
  const trip = page.getByTestId("gesture-trip");
  await expect(trip).toContainText("Trip t10, now 8:00");
  await trip.getByLabel("trip leaves at").fill("7:55");
  await trip.getByRole("button", { name: "Move" }).click();
  await expect(line.locator("g.trip.moved")).toHaveCount(1);
  await expect(page.getByTestId("change-ledger")).toContainText("10");

  // Undo puts the earlier version back; redo returns the change.
  const ledger = page.getByTestId("change-ledger");
  await ledger.getByRole("button", { name: "Undo" }).click();
  await expect(line.locator("g.trip.moved")).toHaveCount(0);
  await ledger.getByRole("button", { name: "Redo" }).click();
  await expect(line.locator("g.trip.moved")).toHaveCount(1);

  // The proposal reads the change route by route.
  await page.keyboard.press("6");
  await expect(page.getByTestId("scorecard")).toContainText("Routes changed");
  await expect(page.getByTestId("redline")).toContainText("10");
  await expect(page.getByTestId("scorecard")).toContainText("D-46");

  // The selection is in the hash: a reload returns to the route.
  await page.keyboard.press("5");
  await page.reload();
  await expect(page.getByTestId("view-route")).toBeVisible();
  await expect(page.getByTestId("string-line")).toContainText("2 trips", { timeout: 10_000 });
});

test("a fork past the calendar carries its weekly service through its period", async ({ page }) => {
  // D-41 (QAF-54): the fixture's calendar ends 2026-12-31. A pick in
  // 2027 forked without a period end runs nothing; with one, the fork
  // carries every weekly calendar to it and the warning never fires.
  await importFixture(page);
  await page.keyboard.press("1");
  await page.getByTestId("fork-name").fill("Late");
  await page.getByTestId("fork-date").fill("2027-01-11");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText("Forked");
  await page.keyboard.press("6");
  await expect(page.getByTestId("readiness")).toContainText(/no service in plan period/i);

  await page.keyboard.press("1");
  await page.getByTestId("fork-name").fill("Winter 2027");
  await page.getByTestId("fork-date").fill("2027-01-11");
  await page.getByTestId("fork-through").fill("2027-05-28");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText("Forked \"Winter 2027\"");
  await page.keyboard.press("6");
  await expect(page.getByTestId("readiness")).not.toContainText(/no service in plan period/i);
});

test("hubs and coverage answer around a station and a layer", async ({ page }) => {
  await importFixture(page);
  await page.getByTestId("group-table").locator("tbody tr", { hasText: "WK" }).click();

  await page.keyboard.press("4");
  const hub = page.getByRole("combobox", { name: "Hub" });
  await hub.fill("Main Station");
  await page.getByRole("option", { name: /Main Station/ }).first().click();
  await expect(page.getByTestId("hub-matrix")).toBeVisible();
  // The station pin departs nothing itself: its waits frame says so
  // (stop_gaps reads one stop, QAF-92); a bay answers.
  await expect(page.getByTestId("unavailable").filter({ hasText: "Waits at Main Station" })).toContainText("QAF-92");
  await hub.fill("Bay A");
  await page.getByRole("option", { name: /Bay A/ }).first().click();
  await expect(page.getByTestId("stop-waits")).toBeVisible();
  await expect(page.getByTestId("corridors")).toContainText("pairs");

  await page.keyboard.press("3");
  await page.getByTestId("layer-input").setInputFiles(path.join(fixtures, "population.geojson"));
  const panel = page.getByTestId("coverage-panel");
  // Coverage is the selected group's (D-55, QAF-89): the WK
  // combination, pooled, named by its members with the dates it holds
  // — no date stands in for it.
  const group = page.getByTestId("group-coverage");
  await expect(group).toContainText("Served by WK");
  await expect(page.getByTestId("coverage-dates")).toContainText("2026-01-01 – 2026-12-31");
  await expect(page.getByTestId("served-pct")).toContainText("%");
  await expect(panel).toContainText("places beyond 400 m");
  // Another group re-asks for its own combination.
  await page.getByTestId("group-select").selectOption("SUN");
  await expect(group).toContainText("Served by SUN");
  await page.getByTestId("group-select").selectOption("WK");
  await expect(group).toContainText("Served by WK");
  // A point layer of one numeric field reads by it, and the answer
  // states its resolution (D-47).
  await expect(page.getByTestId("layer-field")).toHaveValue("POP");
  await expect(page.getByTestId("layer-resolution")).toContainText("Points about");

  // Any weighted geography (D-47): a polygon layer carrying two
  // weights; the planner picks jobs and says what it means.
  await page.getByTestId("layer-input").setInputFiles(path.join(fixtures, "jobs-areas.geojson"));
  await expect(panel).toContainText("2 polygons");
  await page.getByTestId("layer-field").selectOption("jobs");
  await page.getByTestId("layer-label").fill("jobs");
  await page.getByTestId("layer-label").press("Enter");
  await expect(page.getByTestId("layer-resolution")).toContainText("weighted by area");
  await expect(group).toContainText("Served by WK");
  await expect(group.locator("thead")).toContainText("jobs");
});

test("a capability that fails to load fails its frames, not the page", async ({ page }) => {
  // The page's linked assembly does not load (one module: its failure
  // is every capability's — D-77), so each capability falls back to
  // its own module, and analysis's fails too.
  await page.route(/\/caps\/(ui-service-design|analysis)-[0-9a-f]+\.wasm$/, (route) => route.abort());
  await page.goto("/service-design.html");
  await page.getByTestId("design-rail").getByRole("button", { name: "Plans" }).click();
  await page.getByTestId("feed-input").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));

  // The plan set answers (cap-network); the groups frame says why it
  // has none, and offers to try again.
  await expect(page.getByTestId("plan-list")).toContainText("(unframed import)", { timeout: 10_000 });
  const failed = page.getByTestId("unavailable").filter({ hasText: "Service groups" });
  await expect(failed).toContainText("did not load");
  await expect(failed.getByRole("button", { name: "Try again" })).toBeVisible();
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
  await page.goto("/service-design.html#view=route&route=10");
  await page.reload();

  await expect(page.getByTestId("string-line")).toContainText("3 trips", { timeout: 10_000 });
  await page.getByLabel("timetable").check();
  const timetable = page.getByTestId("timetable");
  await expect(timetable.locator("tbody tr")).toHaveCount(3);
  await expect(timetable.locator("tbody tr").first()).toContainText("08:00");
});
