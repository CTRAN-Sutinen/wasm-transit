// An open page follows the deployment (docs/units/deploy-rider-apps.md,
// D-63): a stop display stays open for weeks and a rider's tab for
// days, so on the poll they already run both pages revalidate the
// manifest, and a new network in force — a republish, a new pick, or
// the date crossing a pick's effective date — re-pins WITHOUT a
// reload: the page fetches and registers the new network, its boards
// answer over it, and the old network leaves every module (the
// eviction op's keep-set). Republished over and over, each module's
// registry holds one network and its linear memory plateaus.
//
// Its server is the store-mode one (playwright.config.mjs, :8084,
// today pinned to 2026-08-01); this spec publishes its own pick, dated
// between the other specs' so the page's date resolves to it.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

test.use({ baseURL: "http://127.0.0.1:8084", timezoneId: "UTC" });

/// Publish the import as pick "repin" under `name`: each name is
/// another network (another hash) in force from 2026-11-10.
async function publish(request, name) {
  const { network } = await (await request.get("/network")).json();
  const pick = { ...network, name, plan: { id: "repin", name, effective_from: "2026-11-10" } };
  const resp = await request.post("/plans", { data: JSON.stringify(pick) });
  expect(resp.status(), await resp.text()).toBe(200);
}

async function open(page, url) {
  await page.clock.install({ time: new Date("2026-11-12T07:55:00-05:00") });
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: '{"kind":"edit_records","schema_version":2,"records":[]}' }));
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
  await page.goto(url);
  return (fn, ...args) => page.evaluate(([fn, args]) => window.__bus[fn](...args), [fn, args]);
}

// The store is shared with published-picks.spec.mjs: the pick this
// spec publishes is retired after each test, so it never governs
// another spec's date.
test.afterEach(async ({ request }) => {
  const resp = await request.post("/plans/retire", { data: { id: "repin" } });
  expect(resp.status(), await resp.text()).toBe(200);
});

const networks = async (bus) => (await bus("held", "board")).held.filter((e) => e.kind === "network").map((e) => e.key);

test("an open display and ride-now re-pin a republished network without a reload", async ({ page, context, request }) => {
  // Four republishes, each a poll of both pages.
  test.setTimeout(90_000);
  await publish(request, "Repin 0");
  const display = await open(page, "/stop-display.html?stop=A&layout=routes");
  await expect(page.getByTestId("display-group").first()).toBeVisible({ timeout: 10_000 });
  const rider = await context.newPage();
  const ride = await open(rider, "/ride-now.html");
  await expect(rider.locator("h1")).toHaveText("Repin 0", { timeout: 10_000 });

  // Neither page reloads: a marker set now survives every re-pin.
  for (const p of [page, rider]) await p.evaluate(() => (window.__notReloaded = true));
  const first = await networks(display);
  expect(first).toHaveLength(1);

  const memory = [];
  for (let i = 1; i <= 4; i++) {
    await publish(request, `Repin ${i}`);
    // One poll each: the display's 20 s, ride-now's 30 s.
    await page.clock.fastForward(20_000);
    await rider.clock.fastForward(30_000);
    await expect(rider.locator("h1")).toHaveText(`Repin ${i}`);
    // Settled: the new network registered and the old one evicted.
    await expect
      .poll(async () => {
        const held = await networks(display);
        return held.length === 1 && held[0] !== first[0];
      }, { message: "the old network left the display's module" })
      .toBe(true);
    const held = await networks(display);
    await expect.poll(async () => (await networks(ride)).length, { message: "and ride-now's" }).toBe(1);
    await expect(page.getByTestId("display-group").first()).toBeVisible();
    first.splice(0, 1, ...held);
    memory.push([(await display("memory", "board")).memory, (await ride("memory", "board")).memory]);
  }
  for (const p of [page, rider]) expect(await p.evaluate(() => window.__notReloaded)).toBe(true);
  expect(memory[3], "each module's linear memory plateaus across re-pins").toEqual(memory[1]);
});

test("a poll with nothing moved re-pins nothing", async ({ page, request }) => {
  await publish(request, "Steady");
  const asked = [];
  page.on("request", (r) => asked.push(new URL(r.url()).pathname));
  const bus = await open(page, "/stop-display.html?stop=A&layout=routes");
  await expect(page.getByTestId("display-group").first()).toBeVisible({ timeout: 10_000 });
  const [held] = await networks(bus);
  const fetched = () => asked.filter((p) => p === `/blobs/${held}.gz`).length;
  expect(fetched()).toBe(1);
  const manifests = asked.filter((p) => p === "/manifest").length;
  await page.clock.fastForward(20_000);
  await expect.poll(() => asked.filter((p) => p === "/manifest").length).toBeGreaterThan(manifests);
  expect(fetched(), "the network is not fetched again").toBe(1);
  expect(await networks(bus)).toEqual([held]);
});
