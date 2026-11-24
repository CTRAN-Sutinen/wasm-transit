// The app's build and offline — one lifecycle
// (docs/architecture/updates-offline.md, D-63, D-65;
// web/service-worker.js, kit's "the app's build"):
//
//   1. Both rider pages boot OFFLINE from the service worker — the
//      shell and modules precached, the last network kept — and answer
//      a board from the schedule, marked as the schedule.
//   2. A new build is published: it downloads in the background and
//      waits. A display applies it itself at its next poll while the
//      server answers — never while its setup is open (someone is at
//      the screen). A rider's page offers "a new version is ready" and
//      applies it at the next navigation, never reloading under the
//      rider.
//
// Its server (playwright.config.mjs, :8085) serves a copy of the build
// with the fixture's network; this spec publishes a new build by
// rewriting that copy's sw.js (a byte-different worker is a new build
// to the browser). Service workers are allowed here only.

import { test, expect } from "@playwright/test";
import os from "node:os";
import path from "node:path";
import fs from "node:fs";

const buildsDist = path.join(os.tmpdir(), "transit-e2e-builds");
const swPath = path.join(buildsDist, "sw.js");

test.use({
  baseURL: "http://127.0.0.1:8085",
  serviceWorkers: "allow",
  timezoneId: "UTC",
  geolocation: { latitude: 45.515, longitude: -73.515 },
  permissions: ["geolocation"],
});

/// Open a page, wait until the worker controls it, and load it once
/// more under the worker (so it is the worker that kept what it read).
/// (Polled from here: a page's own timers may be the test's clock.)
async function controlled(page, url) {
  await page.goto(url);
  await expect.poll(() => page.evaluate(() => !!navigator.serviceWorker?.controller), { timeout: 20_000 }).toBe(true);
  await page.reload();
  await expect.poll(() => buildOf(page), { timeout: 20_000 }).toBeTruthy();
  return buildOf(page);
}

const buildOf = (page) => page.evaluate(() => document.documentElement.dataset.build ?? null).catch(() => null);

test("both pages boot offline and answer a board from the schedule", async ({ page, context }) => {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await controlled(page, "/stop-display.html?stop=A&layout=routes");
  await expect(page.locator('[data-testid="display-group"][data-route="10"]').first()).toBeVisible({ timeout: 10_000 });
  const rider = await context.newPage();
  await rider.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await controlled(rider, "/ride-now.html");
  await expect(rider.getByTestId("nearby").getByTestId("stop-card").first()).toContainText("8:15", { timeout: 10_000 });

  await context.setOffline(true);
  await page.reload();
  const ten = page.locator('[data-testid="display-group"][data-route="10"]').first();
  await expect(ten.getByTestId("display-time").first()).toContainText("Scheduled", { timeout: 15_000 });
  await expect(page.getByTestId("display-live")).toContainText("Scheduled times");

  await rider.reload();
  const card = rider.getByTestId("nearby").getByTestId("stop-card").first();
  await expect(card).toContainText("Main Station", { timeout: 15_000 });
  await expect(card).toContainText("8:15");
  await context.setOffline(false);
});

test("a new build: a display applies it at its next poll, a rider's page offers it and applies it at the next navigation", async ({ browser }) => {
  const original = fs.readFileSync(swPath, "utf8");
  const display = await (await browser.newContext({ serviceWorkers: "allow", timezoneId: "UTC" })).newPage();
  const configuring = await (await browser.newContext({ serviceWorkers: "allow", timezoneId: "UTC" })).newPage();
  const rider = await (
    await browser.newContext({ serviceWorkers: "allow", timezoneId: "UTC", geolocation: { latitude: 45.515, longitude: -73.515 }, permissions: ["geolocation"] })
  ).newPage();
  try {
    const at = new Date("2026-10-23T07:55:00-04:00");
    for (const p of [display, configuring, rider]) await p.clock.install({ time: at });
    const before = await controlled(display, "/stop-display.html?stop=A&layout=routes");
    await controlled(configuring, "/stop-display.html");
    await expect(configuring.getByTestId("display-setup")).toBeVisible({ timeout: 10_000 });
    await controlled(rider, "/ride-now.html");
    await expect(rider.getByTestId("nearby").getByTestId("stop-card").first()).toBeVisible({ timeout: 10_000 });
    expect(await buildOf(rider)).toBe(before);
    for (const p of [display, configuring, rider]) await p.evaluate(() => (window.__notReloaded = true));

    // Publish a new build.
    const next = `${before}-next`;
    fs.writeFileSync(swPath, original.replace(`const BUILD = "${before}";`, `const BUILD = "${next}";`));

    // The display: a poll asks for the build, a later poll finds it
    // waiting and applies it — the page reloads into it.
    await expect
      .poll(
        async () => {
          await display.clock.fastForward(20_000).catch(() => {});
          return buildOf(display);
        },
        { timeout: 30_000, intervals: [500] },
      )
      .toBe(next);

    // The display being configured never reloads under its setup.
    for (let i = 0; i < 4; i++) await configuring.clock.fastForward(20_000);
    expect(await configuring.evaluate(() => window.__notReloaded)).toBe(true);
    expect(await buildOf(configuring)).toBe(before);

    // The rider is offered it, and nothing reloads under them.
    await expect
      .poll(
        async () => {
          await rider.clock.fastForward(30_000);
          return rider.getByTestId("update-ready").isVisible();
        },
        { timeout: 30_000, intervals: [500] },
      )
      .toBe(true);
    await expect(rider.getByTestId("update-ready")).toContainText("A new version is ready");
    expect(await rider.evaluate(() => window.__notReloaded)).toBe(true);
    expect(await buildOf(rider)).toBe(before);
    // The next navigation applies it.
    await rider.reload();
    await expect.poll(() => buildOf(rider), { timeout: 20_000 }).toBe(next);
    await expect(rider.getByTestId("update-ready")).toHaveCount(0);
  } finally {
    fs.writeFileSync(swPath, original);
    for (const p of [display, configuring, rider]) await p.context().close();
  }
});

// A staff page is an offline page too (D-189), but its assembly stays
// out of the shell every install downloads (D-223: a rider's install
// never fetches a planner's modules). The page reads its capability
// manifest from the shell, and the worker keeps its assembly on first
// use, hash-named, forever.
test("a staff page's assembly is kept on first use, not precached", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await controlled(page, "/stop-display.html?stop=A&layout=routes");
  const cached = (name) =>
    page.evaluate(async (name) => {
      const out = [];
      for (const key of await caches.keys()) {
        for (const req of await (await caches.open(key)).keys()) if (req.url.includes(name)) out.push(key.startsWith("shell-") ? "shell" : key);
      }
      return out;
    }, name);
  expect(await cached("ui-service-change-")).toEqual([]);
  expect(await cached("ui-stop-display-")).toEqual(["shell"]);
  const manifests = [];
  page.on("response", (r) => {
    if (r.url().includes("caps/manifest.json")) manifests.push({ ui: new URL(r.url()).searchParams.get("ui"), sw: r.fromServiceWorker() });
  });
  await page.goto("/service-change.html");
  await expect.poll(() => manifests.find((m) => m.ui === "service-change"), { timeout: 20_000 }).toEqual({ ui: "service-change", sw: true });
  await expect.poll(() => cached("ui-service-change-"), { timeout: 20_000 }).toEqual(["content-addressed"]);
});

// A staff page is told of a new build and applies it only when the
// person accepts (D-189): a document another build wrote (here an edit
// document one version ahead) makes it look for the deployment's
// build at once; it shows "A new version is ready" and never reloads
// under the person.
test("a staff page refused for its version gets the deployment's build, and applies it only when the person accepts", async ({ page }) => {
  const original = fs.readFileSync(swPath, "utf8");
  try {
    await page.clock.install({ time: new Date("2026-10-23T07:55:00-04:00") });
    const before = await controlled(page, "/disruptions.html");
    await expect(page.getByTestId("clock")).toContainText("7:55");
    await page.evaluate(() => (window.__notReloaded = true));
    fs.writeFileSync(swPath, original.replace(`const BUILD = "${before}";`, `const BUILD = "${before}-next";`));
    await page.route("**/edits", (route) =>
      route.fulfill({ contentType: "application/json", body: JSON.stringify({ kind: "edit_records", schema_version: 99, records: [] }) }),
    );
    // The next read of the edit document meets the newer version.
    await page.clock.fastForward(30_000);
    await expect(page.getByTestId("update-ready")).toContainText("A new version is ready", { timeout: 30_000 });
    expect(await page.evaluate(() => window.__notReloaded)).toBe(true);
    expect(await buildOf(page)).toBe(before);
    await page.getByTestId("update-ready").getByRole("button", { name: "Reload" }).click();
    await expect.poll(() => buildOf(page), { timeout: 20_000 }).toBe(`${before}-next`);
  } finally {
    fs.writeFileSync(swPath, original);
  }
});
