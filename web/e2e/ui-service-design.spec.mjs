// The service-design UI (src/uis/service-design; docs/architecture/web.md;
// docs/units/ui-service-design.md).

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

async function importFixture(page) {
  await page.goto("/service-design.html");
  await page.getByTestId("design-rail").getByRole("button", { name: "Plans" }).click();
  await page.getByTestId("feed-input").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  // HOL runs no trip, so it is in no group.
  await expect(page.getByTestId("group-table").locator("tbody tr")).toHaveCount(2, { timeout: 10_000 });
}

test("service design chains its views over a plan, a fork, and a change", async ({ page }) => {
  await importFixture(page);
  const wk = page.getByTestId("group-table").locator("tbody tr", { hasText: "WK" });
  await expect(wk.getByTestId("group-dates")).toContainText("2026-01-01 – 2026-12-31");
  await wk.click();

  await page.keyboard.press("6");
  await expect(page.getByTestId("readiness").locator("li.warn")).toHaveCount(4);
  await expect(page.getByTestId("readiness")).toContainText("unassigned trips");

  await page.keyboard.press("2");
  await expect(page.getByTestId("group-ledger")).toContainText("Trips");
  await expect(page.getByTestId("ledger-vehicles")).toContainText("2");
  await expect(page.getByTestId("ledger-vehicles")).toContainText("1 blocked + 1 suggested");
  await expect(page.getByTestId("day-profile")).toBeVisible();
  // The profile reads the group's first date.
  await expect(page.getByTestId("day-stats")).toHaveText("Service 8:00 – 8:50 · 5 stops served · 0 transfer points");
  const grid = page.getByTestId("frequency-grid");
  await expect(grid.locator("tbody tr")).toHaveCount(3);

  await grid.locator("tbody tr", { hasText: "10" }).first().click();
  await expect(page.getByTestId("view-route")).toBeVisible();
  await expect(page.getByTestId("string-line")).toContainText("2 trips");
  await expect(page.getByTestId("run-times")).toBeVisible();
  await expect(page.getByTestId("hour-gaps")).toBeVisible();

  await page.getByTestId("edit-toggle").click();
  const gestures = page.getByTestId("gestures");
  await expect(gestures).toContainText("Fork it into a plan");
  await expect(gestures).toContainText("D-54");
  // A gesture changes one service type of the group.
  await expect(page.getByTestId("gesture-scope")).toContainText("Changes the trips of");
  await expect(page.getByTestId("sibling-warning")).toHaveCount(0);

  await page.keyboard.press("1");
  await page.getByTestId("fork-name").fill("Spring");
  await page.getByTestId("fork-date").fill("2026-11-02");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText("Forked");
  await expect(page.getByTestId("pick-timeline")).toContainText("Spring");
  // The view's column does not squeeze the timeline to a line.
  expect((await page.getByTestId("pick-timeline").boundingBox()).height).toBeGreaterThan(60);
  await expect(page.getByTestId("plan-select")).toHaveValue(/plan-/);

  await page.keyboard.press("6");
  const download = page.waitForEvent("download");
  await page.getByTestId("export").click();
  expect((await download).suggestedFilename()).toBe("Spring.zip");

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

  const ledger = page.getByTestId("change-ledger");
  await ledger.getByRole("button", { name: "Undo" }).click();
  await expect(line.locator("g.trip.moved")).toHaveCount(0);
  await ledger.getByRole("button", { name: "Redo" }).click();
  await expect(line.locator("g.trip.moved")).toHaveCount(1);

  // The evict op drops each version that the plans no longer name. A kept
  // version may be absent too: redo answered from the engine's answer cache, and
  // the next ask registers it.
  const bus = (fn, ...args) => page.evaluate(([fn, args]) => window.__bus[fn](...args), [fn, args]);
  const plans = await page.getByTestId("plan-select").locator("option").count();
  const networks = (await bus("held", "author")).held.filter((e) => e.kind === "network");
  expect(networks.length).toBeLessThanOrEqual(plans);

  await page.keyboard.press("6");
  await expect(page.getByTestId("scorecard")).toContainText("Routes changed");
  await expect(page.getByTestId("redline")).toContainText("10");
  await expect(page.getByTestId("score-vehicles")).toContainText("Peak vehicles");
  await expect(page.getByTestId("scorecard")).toContainText("D-108");

  await page.keyboard.press("5");
  await page.reload();
  await expect(page.getByTestId("view-route")).toBeVisible();
  await expect(page.getByTestId("string-line")).toContainText("2 trips", { timeout: 10_000 });
});

test("a change says what it did, in a planner's words", async ({ page }) => {
  await importFixture(page);
  await page.getByTestId("fork-name").fill("Spring");
  await page.getByTestId("fork-date").fill("2026-11-02");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText("Forked \"Spring\" from Base network");

  // The jump box's list floats over the page, and a pick gives the keyboard back.
  const jump = page.getByRole("combobox", { name: "Jump to route" });
  await jump.fill("10");
  const list = page.getByRole("listbox", { name: "Jump to route" });
  expect(await list.evaluate((el) => getComputedStyle(el).position)).toBe("absolute");
  await page.getByRole("option", { name: /^10/ }).first().click();
  await expect(page.getByTestId("string-line")).toContainText("2 trips");
  await expect(jump).not.toBeFocused();
  await page.keyboard.press("e");
  const gestures = page.getByTestId("gestures");
  await expect(gestures).toBeVisible();
  await expect(page.getByTestId("span-now")).toContainText("Now 8:00 to 8:40");

  const line = page.getByTestId("string-line");
  await line.getByLabel("every stop").check();
  await line.getByRole("button", { name: "Main & 3rd" }).click();
  await expect(page.getByTestId("picked-stop")).toContainText("Main & 3rd");

  // t10 moved to 8:15 arrives after its block's next trip leaves at 8:20.
  await line.getByRole("button", { name: /Trip t10 at 8:00/ }).dispatchEvent("pointerdown", { clientX: 0, pointerId: 1 });
  await line.locator("svg").dispatchEvent("pointerup", { pointerId: 1 });
  const trip = page.getByTestId("gesture-trip");
  await trip.getByLabel("trip leaves at").fill("8:15");
  await trip.getByRole("button", { name: "Move" }).click();
  const status = page.getByTestId("status");
  await expect(status).toContainText(/Changed\. The plan now has [1-9]\d* issues?/);
  await expect(status).toHaveClass(/alert/);

  await page.keyboard.press("6");
  const readiness = page.getByTestId("readiness");
  await expect(page.getByTestId("readiness-gate")).toContainText("Not ready");
  await expect(readiness).toContainText("trip t10 arrives at 8:25, after its next trip t20 leaves at 8:20");
  await page.getByTestId("export").click();
  await expect(page.getByTestId("export-refused")).toContainText("Not exported");

  // After the hold, the bus makes the trip. What is left is the recovery that
  // the block's authored layover asks for.
  await readiness.getByRole("button", { name: "Hold trip t20 until 8:25" }).click();
  await expect(readiness).not.toContainText("arrives at 8:25");
  await expect(readiness).toContainText("the wait after trip number 1 of the block is 0 min; 10 min is required there");
  await expect(page.getByTestId("export-refused")).toHaveCount(0);

  await page.keyboard.press("5");
  const ledger = page.getByTestId("change-ledger");
  await ledger.getByRole("button", { name: "Undo" }).click();
  await expect(line.locator("g.trip.moved")).toHaveCount(1);
  await ledger.getByRole("button", { name: "Undo" }).click();
  await expect(status).toContainText("Undone. The plan now has 0 issues");
  await page.keyboard.press("6");
  await expect(page.getByTestId("readiness-gate")).toContainText("Ready, with");
});

test("a fork past the calendar carries its weekly service through its period", async ({ page }) => {
  // The fixture's calendar ends 2026-12-31. A pick in 2027 forked without
  // a period end runs nothing. With one, the fork carries every weekly calendar
  // to it.
  await importFixture(page);
  await page.keyboard.press("1");
  await page.getByTestId("fork-name").fill("Late");
  await page.getByTestId("fork-date").fill("2027-01-11");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText("Forked");
  await page.keyboard.press("6");
  await expect(page.getByTestId("readiness")).toContainText(/no service in plan period/i);

  // A fork keeps only its period's dates, so the carrying fork starts
  // again from the base.
  await page.keyboard.press("1");
  await page.getByTestId("plan-list").locator("tr", { hasText: "the base every fork copies" }).getByRole("button", { name: "Open" }).click();
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
  // stop_gaps pools the stops that name the station as parent.
  const waits = page.getByTestId("stop-waits");
  await expect(waits).toContainText("Waits at Main Station");
  await expect(waits).toContainText("2 departures, 8:15–8:35");
  await expect(waits).toContainText("Longest wait 20 min");
  // Only route 40 serves the station, so nothing just misses here (D-133).
  await expect(page.getByTestId("near-misses")).toContainText("No near misses");
  // Only Main & 1st departs twice on the day (D-134).
  const ranking = page.getByTestId("longest-waits");
  await expect(ranking.locator("tbody tr")).toHaveCount(1);
  await expect(ranking).toContainText("Main & 1st");
  await expect(ranking).toContainText("20 min");
  await ranking.getByRole("button", { name: "Main & 1st" }).click();
  await expect(page.getByTestId("stop-waits")).toContainText("Waits at Main & 1st");
  await hub.fill("Bay A");
  await page.getByRole("option", { name: /Bay A/ }).first().click();
  await expect(page.getByTestId("stop-waits")).toBeVisible();
  await expect(page.getByTestId("corridors")).toContainText("pairs");

  await page.keyboard.press("3");
  await page.getByTestId("layer-input").setInputFiles(path.join(fixtures, "population.geojson"));
  const panel = page.getByTestId("coverage-panel");
  // Coverage is the selected group's.
  const group = page.getByTestId("group-coverage");
  await expect(group).toContainText("Served by WK");
  await expect(page.getByTestId("coverage-dates")).toContainText("2026-01-01 – 2026-12-31");
  await expect(page.getByTestId("served-pct")).toContainText("%");
  await expect(panel).toContainText("places beyond 400 m");
  await page.getByTestId("group-select").selectOption("SUN");
  await expect(group).toContainText("Served by SUN");
  await page.getByTestId("group-select").selectOption("WK");
  await expect(group).toContainText("Served by WK");
  // The answer states its resolution.
  await expect(page.getByTestId("layer-field")).toHaveValue("POP");
  await expect(page.getByTestId("layer-resolution")).toContainText("Points about");

  // A polygon layer carries two weights.
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
  // The linked assembly's failure is every capability's, so each falls back to
  // its own module. The analysis module fails too.
  await page.route(/\/caps\/(ui-service-design|analysis)-[0-9a-f]+\.wasm$/, (route) => route.abort());
  await page.goto("/service-design.html");
  await page.getByTestId("design-rail").getByRole("button", { name: "Plans" }).click();
  await page.getByTestId("feed-input").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));

  // A capability that did not load stays unloaded until the page loads again,
  // so its frame offers no retry. A failed op does.
  await expect(page.getByTestId("plan-list")).toContainText("Base network", { timeout: 10_000 });
  const failed = page.getByTestId("unavailable").filter({ hasText: "Service groups" });
  await expect(failed).toContainText("did not load");
  await expect(failed).toHaveAttribute("data-state", "unassembled");
  await expect(failed.getByRole("button", { name: "Try again" })).toHaveCount(0);
});

// A plan saved before the 2026-10-27 rename (`schedules` / `schedule`) still
// reads, because restore canonicalizes stored documents through cap-author.
test("a plan stored under the old field names restores in the current shape", async ({ page }) => {
  const doc = JSON.parse(fs.readFileSync(path.join(fixtures, "../../../testdata/network.json"), "utf8"));
  const legacy = { ...doc, schedules: {}, blocks: {} };
  delete legacy.pattern_trips;
  // Network v1, written before the tag.
  delete legacy.kind;
  delete legacy.schema_version;
  // The pre-trip shape: exact starts per schedule, and block trips as (schedule, start).
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

  // The device keeps the stored set before the migrated plan overwrites it.
  const kept = await page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const open = indexedDB.open("keyval-store");
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const get = open.result.transaction("keyval").objectStore("keyval").get("caps-documents");
          get.onsuccess = () => resolve(Object.values(get.result?.kinds?.plan_set?.records ?? {}).map((r) => JSON.parse(r.bytes)));
          get.onerror = () => reject(get.error);
        };
      }),
  );
  expect(kept).toEqual([{ legacy }]);
});

// A plan set that a newer build wrote (docs/FORMATS.md, "Versions"). The page
// never writes over it, even when a feed is imported into a plan of its own.
test("a plan set from a newer build is refused and kept as written", async ({ page }) => {
  const newer = { kind: "plan_set", schema_version: 2, plans: { p1: { plan: { id: "from-later" } } } };
  const idb = (op, value) =>
    page.evaluate(
      ([op, value]) =>
        new Promise((resolve, reject) => {
          const open = indexedDB.open("keyval-store");
          open.onupgradeneeded = () => open.result.createObjectStore("keyval");
          open.onerror = () => reject(open.error);
          open.onsuccess = () => {
            const store = open.result.transaction("keyval", "readwrite").objectStore("keyval");
            const req = op === "put" ? store.put(value, "transit:plans") : store.get("transit:plans");
            req.onsuccess = () => resolve(req.result);
            req.onerror = () => reject(req.error);
          };
        }),
      [op, value],
    );
  await page.goto("/service-design.html");
  await idb("put", newer);
  await page.reload();
  await page.getByTestId("design-rail").getByRole("button", { name: "Plans" }).click();
  await page.getByTestId("feed-input").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  await expect(page.getByTestId("plan-list")).toContainText("Base network", { timeout: 10_000 });
  expect(await idb("get")).toEqual(newer);
});
