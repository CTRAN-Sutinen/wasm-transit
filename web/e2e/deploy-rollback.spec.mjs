// Build pointer and rollback (docs/architecture/release-artifacts.md).
// The server on :8086 runs on its own copy of the build. The spec publishes
// release B by rewriting that copy's sw.js, which carries the build's identity.

import { test, expect } from "@playwright/test";
import os from "node:os";
import path from "node:path";
import fs from "node:fs";

const releasesDist = path.join(os.tmpdir(), "transit-e2e-releases");
const swPath = path.join(releasesDist, "sw.js");

test.use({ baseURL: "http://127.0.0.1:8086", serviceWorkers: "allow", timezoneId: "UTC" });

const buildOf = (page) => page.evaluate(() => document.documentElement.dataset.build ?? null).catch(() => null);
const pinned = async (request) => (await (await request.get("/manifest")).json()).build_hash;

/// Each fast-forward is one of the display's 20 s polls.
async function displayRuns(display, build) {
  await expect
    .poll(
      async () => {
        await display.clock.fastForward(20_000).catch(() => {});
        return buildOf(display);
      },
      { timeout: 30_000, intervals: [500] },
    )
    .toBe(build);
}

test("build A, then B, rolled back to A: the display runs A's bytes again", async ({ page: display, request }) => {
  test.setTimeout(90_000);
  const original = fs.readFileSync(swPath, "utf8");
  try {
    const a = await pinned(request);
    expect(a).toMatch(/^sha256:[0-9a-f]{64}$/);
    await display.clock.install({ time: new Date("2026-10-23T07:55:00-04:00") });
    await display.goto("/stop-display.html?stop=A&layout=routes");
    await expect.poll(() => display.evaluate(() => !!navigator.serviceWorker?.controller), { timeout: 20_000 }).toBe(true);
    await display.reload();
    await expect.poll(() => buildOf(display), { timeout: 20_000 }).toBeTruthy();
    const buildA = await buildOf(display);
    await expect(display.locator('[data-testid="display-group"][data-route="10"]').first()).toBeVisible({ timeout: 10_000 });

    const buildB = `${buildA}-b`;
    fs.writeFileSync(swPath, original.replace(`const BUILD = "${buildA}";`, `const BUILD = "${buildB}";`));
    await expect.poll(() => pinned(request), { timeout: 10_000 }).not.toBe(a);
    const b = await pinned(request);
    await displayRuns(display, buildB);

    const resp = await request.post("/builds", { data: { build: a } });
    expect(resp.status(), await resp.text()).toBe(200);
    expect(await pinned(request)).toBe(a);
    expect(await (await request.get("/sw.js")).text(), "the server serves A's bytes").toBe(original);
    await displayRuns(display, buildA);
    // The fast-forwards above can cut off the reloaded page's first reads at
    // their time limits. One more fast-forward lets the retry through.
    await display.clock.fastForward(30_000);
    await expect(display.locator('[data-testid="display-group"][data-route="10"]').first()).toBeVisible({ timeout: 15_000 });

    // B stays retired. Only a pointer move brings it back.
    const log = await (await request.get("/pointers/build/log")).json();
    expect(log.entries.map((e) => e.to)).toEqual([a, b, a]);
    expect(await pinned(request)).toBe(a);
  } finally {
    fs.writeFileSync(swPath, original);
  }
});
