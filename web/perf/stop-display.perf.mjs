// The stop display's perf baseline (docs/TESTING.md, "Performance
// baselines"): the real feed, the agency's recorded live feeds
// replayed one snapshot per poll, and a display left running at a
// transit center — the screen D-64 puts on a Raspberry Pi 4, under
// D-68's budget (wasm + JS heap at most 40 MB, flat across 24 h).
//
//   1. Boot: the setup picks the routes layout and the stop by name;
//      the published network registers and the groups answer live.
//   2. One poll tick, profiled: what the 20 s re-read costs.
//   3. The soak: the display left open while the recording replays
//      PERF_LOOPS times over (default 3 — the clock keeps moving, so a
//      looped snapshot's predictions age; the soak measures what the
//      session holds, not what it shows), memory sampled after a
//      collection every SAMPLE_EVERY ticks, with each module's held
//      documents and resets.
//
// It reports (web/perf-results/<stamp>/); it never fails on a number.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { Probe } from "./probe.mjs";
import { headerSecs, snapshots } from "./live.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const rtDir = path.resolve(root, process.env.PERF_RT ?? "testdata/perf-rt");
const LOOPS = Number(process.env.PERF_LOOPS ?? 3);
const STOP = process.env.PERF_STOP ?? "fisher";
const SAMPLE_EVERY = 10;
const POLL_MS = 20_000;

test("stop display: boot, the poll, the soak", async ({ page }) => {
  const snaps = snapshots(rtDir);
  expect(snaps.length, `recorded snapshots in ${rtDir} (node web/perf/record-rt.mjs)`).toBeGreaterThan(4);
  const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
  const probe = new Probe(page, path.join(root, "web/perf-results", `${stamp}-stop-display`));

  let tick = 0;
  await page.route(/\/rt\?url=/, (route) => {
    const url = decodeURIComponent(route.request().url());
    const kind = /VehiclePositions/i.test(url) ? "vehicles" : "trips";
    const body = snaps[tick % snaps.length][kind];
    return body ? route.fulfill({ contentType: "application/octet-stream", body }) : route.fulfill({ status: 404 });
  });
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: "[]" }));
  await page.clock.install({ time: headerSecs(snaps[0].trips) * 1000 });
  await probe.start();

  async function settle() {
    await page.waitForTimeout(300);
    await page.evaluate(
      () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => requestIdleCallback(() => r(), { timeout: 5000 })))),
    );
  }

  async function pollTick() {
    tick++;
    const live = Promise.all([
      page.waitForResponse((r) => r.url().includes("TripUpdates")),
      page.waitForResponse((r) => r.url().includes("VehiclePositions")),
    ]);
    await page.clock.fastForward(POLL_MS);
    await live;
    await settle();
  }

  // 1. Boot (a same-origin page first, so the profiler follows the
  // renderer across the navigation).
  await page.goto("/live-feeds.json");
  await probe.step("boot: setup → a stop's groups, live", async () => {
    await page.goto("/stop-display.html");
    await expect(page.getByTestId("display-setup")).toBeVisible({ timeout: 120_000 });
    await page.getByTestId("layout-routes").click();
    const input = page.getByLabel("Stop", { exact: true });
    await input.pressSequentially(STOP, { delay: 60 });
    await expect(page.getByRole("option").first()).toBeVisible({ timeout: 30_000 });
    await input.press("Enter");
    await expect(page.getByTestId("display-group").first()).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("display-live")).toContainText("Live", { timeout: 30_000 });
    await settle();
  });
  const stopName = await page.getByTestId("display-stop").textContent();
  await probe.sample("after boot");

  // 2. One poll, profiled.
  await probe.step("poll tick", pollTick);

  // 3. The soak.
  const ticks = [];
  const total = snaps.length * LOOPS;
  while (tick < total - 1) {
    await page.evaluate(() => (window.__perf.longtasks.length = 0));
    const before = await probe.counters();
    await pollTick();
    const after = await probe.counters();
    const longTasks = await page.evaluate(() => window.__perf.longtasks.slice());
    ticks.push({
      tick,
      mainThreadMs: Math.round((after.taskMs - before.taskMs) * 10) / 10,
      scriptMs: Math.round((after.scriptMs - before.scriptMs) * 10) / 10,
      longTasks: longTasks.length,
      longestTaskMs: Math.round(Math.max(0, ...longTasks)),
    });
    if (tick % SAMPLE_EVERY === 0 || tick === total - 1) await probe.sample(`tick ${tick} (${Math.round((tick * POLL_MS) / 60_000)} min)`);
  }
  await expect(page.getByTestId("display-group").first()).toBeVisible();

  const report = probe.write({
    ui: `stop-display (${stopName})`,
    when: new Date().toISOString(),
    feed: path.basename(process.env.PERF_FEED ?? "ctran-gtfs.zip"),
    live: `${path.relative(root, rtDir)} (${snaps.length} snapshots × ${LOOPS} from ${new Date(headerSecs(snaps[0].trips) * 1000).toISOString()})`,
    build: "dist-perf (unminified, wasm name sections)",
    ticks,
  });
  console.log(`perf report: ${path.relative(root, probe.outDir)}/summary.md (${report.steps.length} steps, ${ticks.length} soak ticks)`);
});
