// App build and offline lifecycle (docs/architecture/updates-offline.md).
// The server on :8085 serves a copy of the build, and the spec
// publishes a new build by rewriting that copy's sw.js. A byte-different
// worker is a new build to the browser. Only this spec allows service workers.

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

/// The second load runs under the worker, so the worker kept what the page read.
/// The poll runs here because a page's own timers may be the test's clock.
async function controlled(page, url) {
  await page.goto(url);
  await expect
    .poll(() => page.evaluate(() => !!navigator.serviceWorker?.controller), {
      timeout: 20_000,
    })
    .toBe(true);
  await page.reload();
  await expect.poll(() => buildOf(page), { timeout: 20_000 }).toBeTruthy();
  return buildOf(page);
}

const buildOf = (page) =>
  page
    .evaluate(() => document.documentElement.dataset.build ?? null)
    .catch(() => null);

test("both pages boot offline and answer a board from the schedule", async ({
  page,
  context,
}) => {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await controlled(page, "/stop-display.html?stop=A&layout=routes");
  await expect(
    page.locator('[data-testid="display-group"][data-route="10"]').first(),
  ).toBeVisible({ timeout: 10_000 });
  const rider = await context.newPage();
  await rider.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await controlled(rider, "/ride-now.html");
  await expect(
    rider.getByTestId("nearby").getByTestId("stop-card").first(),
  ).toContainText("8:15", { timeout: 10_000 });

  await context.setOffline(true);
  await page.reload();
  const ten = page
    .locator('[data-testid="display-group"][data-route="10"]')
    .first();
  await expect(ten.getByTestId("display-time").first()).toContainText(
    "Scheduled",
    { timeout: 15_000 },
  );
  await expect(page.getByTestId("display-live")).toContainText(
    "Scheduled times",
  );

  await rider.reload();
  const card = rider.getByTestId("nearby").getByTestId("stop-card").first();
  await expect(card).toContainText("Main Station", { timeout: 15_000 });
  await expect(card).toContainText("8:15");
  await context.setOffline(false);
});

test("a new build: a display applies it at its next poll, a rider's page offers it and applies it at the next navigation", async ({
  browser,
}) => {
  const original = fs.readFileSync(swPath, "utf8");
  const display = await (
    await browser.newContext({ serviceWorkers: "allow", timezoneId: "UTC" })
  ).newPage();
  const configuring = await (
    await browser.newContext({ serviceWorkers: "allow", timezoneId: "UTC" })
  ).newPage();
  const rider = await (
    await browser.newContext({
      serviceWorkers: "allow",
      timezoneId: "UTC",
      geolocation: { latitude: 45.515, longitude: -73.515 },
      permissions: ["geolocation"],
    })
  ).newPage();
  try {
    const at = new Date("2026-10-23T07:55:00-04:00");
    for (const p of [display, configuring, rider])
      await p.clock.install({ time: at });
    const before = await controlled(
      display,
      "/stop-display.html?stop=A&layout=routes",
    );
    await controlled(configuring, "/stop-display.html");
    await expect(configuring.getByTestId("display-setup")).toBeVisible({
      timeout: 10_000,
    });
    await controlled(rider, "/ride-now.html");
    await expect(
      rider.getByTestId("nearby").getByTestId("stop-card").first(),
    ).toBeVisible({ timeout: 10_000 });
    expect(await buildOf(rider)).toBe(before);
    for (const p of [display, configuring, rider])
      await p.evaluate(() => (window.__notReloaded = true));

    const next = `${before}-next`;
    fs.writeFileSync(
      swPath,
      original.replace(
        `const BUILD = "${before}";`,
        `const BUILD = "${next}";`,
      ),
    );

    await expect
      .poll(
        async () => {
          await display.clock.fastForward(20_000).catch(() => {});
          return buildOf(display);
        },
        { timeout: 30_000, intervals: [500] },
      )
      .toBe(next);

    for (let i = 0; i < 4; i++) await configuring.clock.fastForward(20_000);
    expect(await configuring.evaluate(() => window.__notReloaded)).toBe(true);
    expect(await buildOf(configuring)).toBe(before);

    await expect
      .poll(
        async () => {
          await rider.clock.fastForward(30_000);
          return rider.getByTestId("update-ready").isVisible();
        },
        { timeout: 30_000, intervals: [500] },
      )
      .toBe(true);
    await expect(rider.getByTestId("update-ready")).toContainText(
      "A new version is ready",
    );
    expect(await rider.evaluate(() => window.__notReloaded)).toBe(true);
    expect(await buildOf(rider)).toBe(before);
    await rider.reload();
    await expect.poll(() => buildOf(rider), { timeout: 20_000 }).toBe(next);
    await expect(rider.getByTestId("update-ready")).toHaveCount(0);
  } finally {
    fs.writeFileSync(swPath, original);
    for (const p of [display, configuring, rider]) await p.context().close();
  }
});

// A staff page works offline. Its assembly stays out of the shell, so
// a rider's install never fetches a planner's modules (D-223). The worker
// keeps the assembly on first use, hash-named.
test("a staff page's assembly is kept on first use, not precached", async ({
  page,
}) => {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await controlled(page, "/stop-display.html?stop=A&layout=routes");
  const cached = (name) =>
    page.evaluate(async (name) => {
      const out = [];
      for (const key of await caches.keys()) {
        for (const req of await (await caches.open(key)).keys())
          if (req.url.includes(name))
            out.push(key.startsWith("shell-") ? "shell" : key);
      }
      return out;
    }, name);
  expect(await cached("ui-service-change-")).toEqual([]);
  expect(await cached("ui-stop-display-")).toEqual(["shell"]);
  const manifests = [];
  page.on("response", (r) => {
    if (r.url().includes("caps/manifest.json"))
      manifests.push({
        ui: new URL(r.url()).searchParams.get("ui"),
        sw: r.fromServiceWorker(),
      });
  });
  await page.goto("/service-change.html");
  await expect
    .poll(() => manifests.find((m) => m.ui === "service-change"), {
      timeout: 20_000,
    })
    .toEqual({ ui: "service-change", sw: true });
  await expect
    .poll(() => cached("ui-service-change-"), { timeout: 20_000 })
    .toEqual(["content-addressed"]);
});

// A staff page applies a new build only when the person accepts. A
// document that a newer build wrote makes the page look for the build at once.
test("a staff page refused for its version gets the deployment's build, and applies it only when the person accepts", async ({
  page,
}) => {
  const original = fs.readFileSync(swPath, "utf8");
  try {
    await page.clock.install({ time: new Date("2026-10-23T07:55:00-04:00") });
    const before = await controlled(page, "/disruptions.html");
    await expect(page.getByTestId("clock")).toContainText("7:55");
    await page.evaluate(() => (window.__notReloaded = true));
    fs.writeFileSync(
      swPath,
      original.replace(
        `const BUILD = "${before}";`,
        `const BUILD = "${before}-next";`,
      ),
    );
    await page.route("**/edits", (route) =>
      route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          kind: "edit_records",
          schema_version: 99,
          records: [],
        }),
      }),
    );
    await page.clock.fastForward(30_000);
    await expect(page.getByTestId("update-ready")).toContainText(
      "A new version is ready",
      { timeout: 30_000 },
    );
    expect(await page.evaluate(() => window.__notReloaded)).toBe(true);
    expect(await buildOf(page)).toBe(before);
    await page
      .getByTestId("update-ready")
      .getByRole("button", { name: "Reload" })
      .click();
    await expect
      .poll(() => buildOf(page), { timeout: 20_000 })
      .toBe(`${before}-next`);
  } finally {
    fs.writeFileSync(swPath, original);
  }
});
