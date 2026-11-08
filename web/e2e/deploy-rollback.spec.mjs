// Every build kept, and rolling code back (docs/units/release-artifacts.md,
// D-80; docs/ARCHITECTURE.md, "Release artifacts"): a store-mode
// server commits the release in its -dist to the store's `build`
// pointer and serves the pages from the pinned build. A new release is
// a new build the manifest pins, and an open display applies it at a
// poll (D-63). Rolling back is the pointer moved back to a build the
// log holds (POST /builds): the manifest moves, the server serves the
// old build's bytes, and the display runs them again.
//
// Its server (playwright.config.mjs, :8086) runs on its own copy of the
// build; this spec publishes release B by rewriting that copy's sw.js
// (the worker carries the build's identity).

import { test, expect } from "@playwright/test";
import os from "node:os";
import path from "node:path";
import fs from "node:fs";

const releasesDist = path.join(os.tmpdir(), "transit-e2e-releases");
const swPath = path.join(releasesDist, "sw.js");

test.use({ baseURL: "http://127.0.0.1:8086", serviceWorkers: "allow", timezoneId: "UTC" });

const buildOf = (page) => page.evaluate(() => document.documentElement.dataset.build ?? null).catch(() => null);
const pinned = async (request) => (await (await request.get("/manifest")).json()).build_hash;

/// Poll the display (each fast-forward is one of its 20 s polls) until
/// it runs `build`.
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
    // Build A: the release the server started on, pinned by the manifest.
    const a = await pinned(request);
    expect(a).toMatch(/^sha256:[0-9a-f]{64}$/);
    await display.clock.install({ time: new Date("2026-10-23T07:55:00-04:00") });
    await display.goto("/stop-display.html?stop=A&layout=routes");
    await expect.poll(() => display.evaluate(() => !!navigator.serviceWorker?.controller), { timeout: 20_000 }).toBe(true);
    await display.reload();
    await expect.poll(() => buildOf(display), { timeout: 20_000 }).toBeTruthy();
    const buildA = await buildOf(display);
    await expect(display.locator('[data-testid="display-group"][data-route="10"]').first()).toBeVisible({ timeout: 10_000 });

    // Release B lands: the server commits it, the manifest pins it, and
    // the display applies it at a poll.
    const buildB = `${buildA}-b`;
    fs.writeFileSync(swPath, original.replace(`const BUILD = "${buildA}";`, `const BUILD = "${buildB}";`));
    await expect.poll(() => pinned(request), { timeout: 10_000 }).not.toBe(a);
    const b = await pinned(request);
    await displayRuns(display, buildB);

    // Roll back: the pointer moves to A — a build the log holds.
    const resp = await request.post("/builds", { data: { build: a } });
    expect(resp.status(), await resp.text()).toBe(200);
    expect(await pinned(request)).toBe(a);
    expect(await (await request.get("/sw.js")).text(), "the server serves A's bytes").toBe(original);
    await displayRuns(display, buildA);
    await expect(display.locator('[data-testid="display-group"][data-route="10"]').first()).toBeVisible();

    // B is still in -dist, and stays retired: the log holds it, so only
    // a pointer move brings it back.
    const log = await (await request.get("/pointers/build/log")).json();
    expect(log.entries.map((e) => e.to)).toEqual([a, b, a]);
    expect(await pinned(request)).toBe(a);
  } finally {
    fs.writeFileSync(swPath, original);
  }
});
