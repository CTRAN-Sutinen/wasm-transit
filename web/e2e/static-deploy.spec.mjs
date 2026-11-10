// Static deployments, one per UI (D-131; docs/architecture/static-deployment.md): the server's export of ONE UI — its
// page, the manifest, and every published document as
// blobs/<hash>[.gz] — served by a plain file server
// (e2e/static-server.mjs: no negotiation, no ETags, no routes) under a
// path prefix. No Go server runs. Ride-now (/ride/) and the stop
// display (/display/) are two deployments, each carrying only its own
// page and what it runs. The pages boot, answer, re-pin a redeploy, and
// boot offline exactly as they do over the sync server, through the
// same layout; what a static host has not (the edit document, a
// geocoder, the dev relay) is absent, not an error.
//
// Its host (playwright.config.mjs, :8087) serves the two exports of the
// fixture feed and the walking fixture's streets, with a
// live-feeds.json naming feeds.test directly (no proxy); a third export
// (the stop display over the overnight feed) waits beside them for the
// redeploy.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import os from "node:os";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");
const ride = path.join(os.tmpdir(), "transit-e2e-static", "ride");
const display = path.join(os.tmpdir(), "transit-e2e-static", "display");
const next = path.join(os.tmpdir(), "transit-e2e-static-next");
const manifest = (bundle) => JSON.parse(fs.readFileSync(path.join(bundle, "manifest"), "utf8"));

test.use({
  baseURL: "http://127.0.0.1:8087",
  timezoneId: "UTC",
  geolocation: { latitude: 45.515, longitude: -73.515 },
  permissions: ["geolocation"],
});

/// The deployment's feeds, answered directly (CORS, as D-67 asks of
/// the agency).
async function feeds(page) {
  await page.route("http://feeds.test/**", (route) =>
    route.fulfill({
      contentType: "application/octet-stream",
      headers: { "Access-Control-Allow-Origin": "*" },
      body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")),
    }),
  );
}

/// What the page asked its own origin for, by path.
function requests(page) {
  const paths = [];
  page.on("request", (r) => {
    const url = new URL(r.url());
    if (url.port === "8087") paths.push(url.pathname);
  });
  return paths;
}

const networks = async (page) =>
  (await page.evaluate(() => window.__bus.held("board"))).held.filter((e) => e.kind === "network").map((e) => e.key);

test("ride now boots from a static host under a prefix", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await feeds(page);
  const asked = requests(page);
  await page.goto("/ride/ride-now.html");

  const first = page.getByTestId("nearby").getByTestId("stop-card").first();
  await expect(first).toContainText("Main Station", { timeout: 10_000 });
  await expect(first).toContainText("8:15");

  // Every published document by its hash, under the prefix, gzipped
  // where a module registers it; nothing outside the bundle.
  const m = manifest(ride);
  await expect
    .poll(() => asked)
    .toEqual(
      expect.arrayContaining([
        "/ride/manifest",
        `/ride/blobs/${m.network_hash}.gz`,
        `/ride/blobs/${m.boundary_hash}`,
        `/ride/blobs/${m.streets_hash}.gz`,
        `/ride/blobs/${m.walks_hash}`,
      ]),
    );
  expect(asked.filter((p) => !p.startsWith("/ride/")), "every fetch stays inside the bundle").toEqual([]);

  // No geocoder in the manifest: place search finds stops only.
  await expect(page.getByPlaceholder("Search for a stop", { exact: true })).toBeVisible();
});

test.describe("ride now plans a trip from a static host", () => {
  // ~22 m north of Station Rd, west of the bays (the walking fixture).
  test.use({ geolocation: { latitude: 45.5152, longitude: -73.5165 } });

  test("directions to a stop walk the published streets", async ({ page }) => {
    await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
    await feeds(page);
    await page.goto("/ride/ride-now.html");
    const cards = page.getByTestId("nearby").getByTestId("stop-card");
    await expect(cards.first().getByTestId("walk")).toHaveText("1 min walk", { timeout: 10_000 });
    await cards.first().getByRole("button").first().click();
    await page.getByTestId("rider-board").getByRole("button", { name: "Directions to here" }).click();
    await expect(page.getByLabel("From", { exact: true })).toHaveValue("My location");
    await expect(page.getByTestId("plan-now")).toContainText("min walk to Main Station");
  });
});

test("the stop display answers live from a static host with no edit document", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await feeds(page);
  const asked = requests(page);
  await page.goto("/display/stop-display.html?stop=A&layout=routes");

  await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 10_000 });
  const ten = page.locator('[data-testid="display-group"][data-route="10"]').first();
  await expect(ten.getByTestId("display-time").first()).toContainText("2 min late");
  // The host has no edit document: asked, absent, and the board runs.
  await expect.poll(() => asked).toContain("/display/edits");
});

test("each deployment carries its own UI and nothing else", async ({ page, request }) => {
  // The other UIs' pages are not there, nor service design anywhere.
  for (const absent of ["/ride/stop-display.html", "/ride/service-design.html", "/display/ride-now.html", "/display/service-design.html"]) {
    expect((await request.get(absent)).status(), absent).toBe(404);
  }
  // The stop display draws no map: no basemap in its deployment.
  expect((await request.get("/display/basemap-assets/sprites/v4/light.json")).status()).toBe(404);
  // Each carries its own assembly, not the other's.
  const caps = (bundle) => fs.readdirSync(path.join(bundle, "caps"));
  expect(caps(ride).some((f) => f.startsWith("ui-ride-now-"))).toBe(true);
  expect(caps(ride).some((f) => f.startsWith("ui-stop-display-") || f.startsWith("ui-service-design-"))).toBe(false);
  expect(caps(display).some((f) => f.startsWith("ui-stop-display-"))).toBe(true);
  expect(caps(display).some((f) => f.startsWith("ui-ride-now-") || f.startsWith("ui-service-design-"))).toBe(false);
  // The deployment's root opens its UI, keeping the query.
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await feeds(page);
  await page.goto("/display/?stop=A&layout=routes");
  await expect(page).toHaveURL(/\/display\/stop-display\.html\?stop=A&layout=routes$/);
  await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 10_000 });
});

test.describe("offline", () => {
  test.use({ serviceWorkers: "allow" });

  test("the stop display boots offline from a static host", async ({ page, context }) => {
    await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
    await feeds(page);
    await page.goto("/display/stop-display.html?stop=A&layout=routes");
    await expect.poll(() => page.evaluate(() => !!navigator.serviceWorker?.controller), { timeout: 20_000 }).toBe(true);
    // Once more under the worker, so it is the worker that kept what it read.
    await page.reload();
    await expect(page.locator('[data-testid="display-group"][data-route="10"]').first()).toBeVisible({ timeout: 10_000 });

    await context.setOffline(true);
    await page.reload();
    await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 10_000 });
    await expect(page.locator('[data-testid="display-group"][data-route="10"]').first()).toBeVisible();
    await context.setOffline(false);
  });

  test("ride now boots offline from a static host", async ({ page, context }) => {
    await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
    await feeds(page);
    await page.goto("/ride/ride-now.html");
    await expect.poll(() => page.evaluate(() => !!navigator.serviceWorker?.controller), { timeout: 20_000 }).toBe(true);
    await page.reload();
    const first = page.getByTestId("nearby").getByTestId("stop-card").first();
    await expect(first).toContainText("8:15", { timeout: 10_000 });

    await context.setOffline(true);
    await page.reload();
    await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toContainText("8:15", { timeout: 10_000 });
    await context.setOffline(false);
  });
});

test("an open display re-pins a redeploy without a reload", async ({ page }) => {
  const bundle = display;
  const original = fs.readFileSync(path.join(bundle, "manifest"));
  try {
    await page.clock.install({ time: new Date("2026-10-23T07:55:00-04:00") });
    await feeds(page);
    await page.goto("/display/stop-display.html?stop=A&layout=routes");
    await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 10_000 });
    await page.evaluate(() => (window.__notReloaded = true));
    const [held] = await networks(page);
    expect(held).toBe(manifest(bundle).network_hash);

    // The redeploy, as an upload does it: the new bundle's blobs beside
    // the old ones (create-only — an open page pinned to the old
    // manifest still finds its documents), then its manifest.
    for (const blob of fs.readdirSync(path.join(next, "blobs"))) {
      const to = path.join(bundle, "blobs", blob);
      if (!fs.existsSync(to)) fs.copyFileSync(path.join(next, "blobs", blob), to);
    }
    fs.copyFileSync(path.join(next, "manifest"), path.join(bundle, "manifest"));
    const moved = manifest(bundle).network_hash;
    expect(moved).not.toBe(held);

    // One poll (the display's 20 s): the new network, the old evicted.
    await page.clock.fastForward(20_000);
    await expect.poll(() => networks(page), { timeout: 10_000 }).toEqual([moved]);
    expect(await page.evaluate(() => window.__notReloaded)).toBe(true);
  } finally {
    fs.writeFileSync(path.join(bundle, "manifest"), original);
  }
});
