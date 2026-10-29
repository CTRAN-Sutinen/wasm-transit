// The ride-now UI's perf baseline (docs/TESTING.md, "Performance
// baselines"): the real feed, the agency's recorded live feeds replayed
// one snapshot per poll, and a rider in downtown Vancouver.
//
//   1. Boot: the server's published network registers, near you
//      answers with live predictions.
//   2. A rider's session: type a stop, pick a departure, save the stop,
//      plan a trip (now, the last bus, arrive by), explore a route,
//      track a bus — each step timed and profiled.
//   3. One poll tick on each live view (trip "now", bus, home),
//      profiled: what the 30 s re-read costs where it costs most.
//   4. The soak: home left open while every remaining snapshot
//      arrives (the clock fast-forwards a poll at a time); memory is
//      sampled after a collection every SAMPLE_EVERY ticks.
//
// It reports (web/perf-results/<stamp>/: summary.md, report.json, the
// .cpuprofile files — open them in DevTools' Performance panel); it
// never fails on a number.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";
import { Probe } from "./probe.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const rtDir = path.resolve(root, process.env.PERF_RT ?? "testdata/perf-rt");
const SAMPLE_EVERY = 10;
const POLL_MS = 30_000;

test.use({
  geolocation: { latitude: 45.6266, longitude: -122.6746 },
  permissions: ["geolocation"],
});

/// The recorded snapshots, in order: [{ trips, vehicles }] as bytes.
function snapshots() {
  const byN = new Map();
  for (const f of fs.readdirSync(rtDir).sort()) {
    const m = /^(\d+)-(\w+)\.pb$/.exec(f);
    if (!m) continue;
    if (!byN.has(m[1])) byN.set(m[1], {});
    byN.get(m[1])[m[2]] = fs.readFileSync(path.join(rtDir, f));
  }
  return [...byN.values()];
}

/// The FeedHeader's timestamp (FeedMessage field 1, header field 3):
/// the clock starts where the recording did.
function headerSecs(buf) {
  const varint = (i) => {
    let x = 0, s = 1;
    for (;;) {
      const c = buf[i++];
      x += (c & 0x7f) * s;
      s *= 128;
      if (!(c & 0x80)) return [x, i];
    }
  };
  let [tag, i] = varint(0);
  if (tag !== 0x0a) throw new Error("no FeedHeader");
  let len;
  [len, i] = varint(i);
  const end = i + len;
  while (i < end) {
    let t, v;
    [t, i] = varint(i);
    if ((t & 7) === 0) {
      [v, i] = varint(i);
      if (t >> 3 === 3) return v;
    } else if ((t & 7) === 2) {
      [v, i] = varint(i);
      i += v;
    } else throw new Error(`unexpected wire type in FeedHeader`);
  }
  throw new Error("FeedHeader has no timestamp");
}

test("ride now: boot, a rider's session, the poll, the soak", async ({ page }) => {
  const snaps = snapshots();
  expect(snaps.length, `recorded snapshots in ${rtDir} (node web/perf/record-rt.mjs)`).toBeGreaterThan(4);
  const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
  const probe = new Probe(page, path.join(root, "web/perf-results", stamp));

  // The live feeds: snapshot `tick` for every request until the soak
  // moves it.
  let tick = 0;
  await page.route(/\/rt\?url=/, (route) => {
    const url = decodeURIComponent(route.request().url());
    const kind = /VehiclePositions/i.test(url) ? "vehicles" : "trips";
    const body = snaps[Math.min(tick, snaps.length - 1)][kind];
    return body ? route.fulfill({ contentType: "application/octet-stream", body }) : route.fulfill({ status: 404 });
  });
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: "[]" }));
  await page.clock.install({ time: headerSecs(snaps[0].trips) * 1000 });
  await probe.start();

  /// One poll: the next snapshot, the clock forward to the timer, the
  /// UI settled on the answers.
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

  /// Quiet: two frames painted and the main thread idle.
  async function settle() {
    await page.waitForTimeout(300);
    await page.evaluate(
      () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => requestIdleCallback(() => r(), { timeout: 5000 })))),
    );
  }

  async function pick(field, text, name) {
    const input = page.getByLabel(field, { exact: true });
    await input.fill("");
    await input.pressSequentially(text, { delay: 60 });
    await expect(page.getByRole("option").first()).toContainText(name, { timeout: 15_000 });
    await input.press("Enter");
  }

  async function home() {
    const routes = page.getByRole("button", { name: "Routes" });
    for (let i = 0; i < 4 && !(await routes.isVisible()); i++) await page.getByRole("button", { name: "back" }).first().click();
    await expect(routes).toBeVisible();
  }

  const board = page.getByTestId("rider-board");

  // 1. Boot (a same-origin page first, so the profiler follows the
  // renderer across the navigation).
  await page.goto("/live-feeds.json");
  await probe.step("boot: network registered, near you live", async () => {
    await page.goto("/ride-now.html");
    await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toBeVisible({ timeout: 120_000 });
    await expect(page.getByTestId("live-feeds")).toContainText("Live", { timeout: 30_000 });
    await settle();
  });
  await probe.sample("after boot");

  // 2. A rider's session.
  await probe.step("type-ahead → stop board", async () => {
    await pick("stop name", "fisher", "Fisher");
    await expect(board.locator("h2")).toBeVisible();
    await expect(board.getByTestId("board-rows").locator("li").first()).toBeVisible();
    await settle();
  });

  await probe.step("pick a departure (route on the map)", async () => {
    await board.getByTestId("board-rows").locator("li button").first().click();
    await settle();
  });

  await probe.step("save the stop", async () => {
    await board.getByRole("button", { name: "Save this stop" }).click();
    await expect(board.getByRole("button", { name: "Remove from saved" })).toBeVisible();
    await settle();
  });

  const trip = page.getByTestId("trip-plan");
  const itinerary = trip.getByTestId("itinerary");
  await probe.step("plan a trip leaving now", async () => {
    await board.getByRole("button", { name: "Directions from here" }).click();
    await pick("To", "99th street transit", "99th Street");
    await expect(itinerary).toBeVisible({ timeout: 30_000 });
    await settle();
  });

  await probe.step("poll tick on trip (now re-plans)", pollTick);

  await probe.step("last bus", async () => {
    await trip.getByRole("radio", { name: "Last bus" }).click();
    await expect(trip.getByTestId("last-bus")).toBeVisible({ timeout: 30_000 });
    await settle();
  });

  await probe.step("arrive by 17:30", async () => {
    await trip.getByRole("radio", { name: "Arrive by" }).click();
    await trip.locator("input[type=time]").fill("17:30");
    await expect(itinerary).toBeVisible({ timeout: 30_000 });
    await settle();
  });

  await home();
  await probe.step("routes list → filter → a route", async () => {
    await page.getByRole("button", { name: "Routes" }).click();
    await page.getByLabel("filter routes").pressSequentially("vine", { delay: 60 });
    await page.getByTestId("route-list").locator("li button").first().click();
    await expect(page.getByTestId("route-view").getByTestId("route-variant").first()).toBeVisible();
    await settle();
  });

  await probe.step("a stop on the route → its board", async () => {
    await page.getByTestId("route-variant").first().locator(".line button").first().click();
    await expect(board.getByTestId("board-rows").locator("li").first()).toBeVisible();
    await settle();
  });

  // A departure whose bus is reporting, if one of the first few is.
  const rows = board.getByTestId("board-rows").locator("li button");
  let tracking = false;
  for (let i = 0; i < Math.min(6, await rows.count()) && !tracking; i++) {
    await rows.nth(i).click();
    tracking = await board
      .getByRole("button", { name: "Track this bus ›" })
      .waitFor({ timeout: 2_000 })
      .then(() => true, () => false);
  }
  if (tracking) {
    await probe.step("track a bus", async () => {
      await board.getByRole("button", { name: "Track this bus ›" }).click();
      await expect(page.getByTestId("bus-view").getByTestId("bus-stops").locator("li").first()).toBeVisible();
      await settle();
    });
    await probe.step("poll tick on bus view", pollTick);
  }

  await home();
  await probe.step("poll tick on home (near you + saved)", pollTick);
  await probe.sample(`tick ${tick} (after the session)`);

  // 4. The soak: home open, every remaining snapshot.
  const ticks = [];
  while (tick < snaps.length - 1) {
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
    if (tick % SAMPLE_EVERY === 0 || tick === snaps.length - 1) await probe.sample(`tick ${tick} (${Math.round((tick * POLL_MS) / 60_000)} min)`);
  }

  const report = probe.write({
    ui: "ride-now",
    when: new Date().toISOString(),
    feed: path.basename(process.env.PERF_FEED ?? "ctran-gtfs.zip"),
    live: `${path.relative(root, rtDir)} (${snaps.length} snapshots from ${new Date(headerSecs(snaps[0].trips) * 1000).toISOString()})`,
    build: "dist-perf (unminified, wasm name sections)",
    ticks,
  });
  console.log(`perf report: ${path.relative(root, probe.outDir)}/summary.md (${report.steps.length} steps, ${ticks.length} soak ticks)`);
});
