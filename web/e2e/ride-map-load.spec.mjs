// How the ride map loads (src/viewport.ts, src/basemap.ts): it opens
// on the view the rider will end up on, so the camera makes no move
// whose tiles are thrown away, and it asks for its tile archive once.
//
//   1. A rider who shared a location before opens on it: the camera
//      starts at the last fix, and neither framing the network nor
//      the fix's arrival moves it — the camera makes no move at all.
//   2. A first visit with a fix pending goes to the fix in ONE move —
//      the network is not framed on the way.
//   3. A rider who shares no location sees the network framed, once —
//      and the next load opens on that frame and does not move.
//   4. The archive's header is fetched once, with no probe before it.
//
// 1–3 run on the fallback style (the archive stubbed away, OpenFreeMap
// stubbed to an empty style) so they need neither; 4 needs the local
// basemap.pmtiles and skips without it.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const webDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
// Main Station, inside the fixture network (stops A..C span 45.50–45.52,
// -73.52..-73.50; their center is ~1.4 km from here).
const fix = { latitude: 45.515, longitude: -73.515 };
const near = (a, b) => Math.abs(a - b) < 0.002;

// The server that publishes the fixture network (:8083, as
// published-network.spec.mjs): the map has stops to frame.
test.use({ baseURL: "http://127.0.0.1:8083", timezoneId: "UTC", geolocation: fix, permissions: ["geolocation"] });

/// The live feeds and the edit document the page polls, answered so
/// the spec is the only thing on the wire it looks at.
async function quiet(page) {
  await page.route(/\/rt\?url=/, (route) => route.fulfill({ status: 404 }));
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: "[]" }));
}

/// The fallback style, hermetic: no archive, and OpenFreeMap's style
/// is an empty one (nothing to fetch, so the map loads offline).
async function fallbackStyle(page) {
  await page.route("**/basemap.pmtiles", (route) => route.fulfill({ status: 404 }));
  await page.route("https://tiles.openfreemap.org/**", (route) =>
    route.fulfill({ contentType: "application/json", body: JSON.stringify({ version: 8, sources: {}, layers: [] }) }),
  );
}

/// Record the camera: where the map opened (`open`, when the page
/// gets its instance) and where every move ended.
async function recordCamera(page) {
  await page.addInitScript(() => {
    window.__camera = [];
    let map;
    Object.defineProperty(window, "__map", {
      configurable: true,
      get: () => map,
      set(m) {
        map = m;
        const at = (when) => ({ when, lon: m.getCenter().lng, lat: m.getCenter().lat, zoom: m.getZoom() });
        window.__camera.push(at("open"));
        m.on("moveend", () => window.__camera.push(at("moveend")));
      },
    });
  });
}

/// The camera, once the stops are drawn and the page has had time to
/// settle (the fix, delayed or not, has arrived and been acted on).
async function settledCamera(page) {
  await expect(page.getByTestId("ride-map")).not.toHaveAttribute("data-stops", "0", { timeout: 10_000 });
  await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toBeVisible({ timeout: 10_000 });
  await page.waitForTimeout(800);
  return page.evaluate(() => window.__camera);
}

/// A fix that takes `ms` to arrive, as a phone's first one can.
async function slowFix(page, ms) {
  await page.addInitScript((ms) => {
    const ask = navigator.geolocation.getCurrentPosition.bind(navigator.geolocation);
    navigator.geolocation.getCurrentPosition = (ok, err, opts) => setTimeout(() => ask(ok, err, opts), ms);
  }, ms);
}

test("a rider who shared a location before opens on it and the camera never leaves", async ({ page }) => {
  await quiet(page);
  await fallbackStyle(page);
  // The first visit: the fix is shared and remembered.
  await page.goto("/ride-now.html");
  await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toBeVisible({ timeout: 10_000 });

  // The next one, the fix slow to arrive.
  await recordCamera(page);
  await slowFix(page, 2000);
  await page.reload();
  const camera = await settledCamera(page);

  expect(camera[0].when).toBe("open");
  expect(near(camera[0].lat, fix.latitude) && near(camera[0].lon, fix.longitude)).toBe(true);
  expect(camera[0].zoom).toBe(15.5);
  expect(camera.filter((c) => c.when === "moveend"), "no move at all: the fix is where it opened").toEqual([]);
});

test("a first visit with a fix pending goes to the fix in one move", async ({ page }) => {
  await quiet(page);
  await fallbackStyle(page);
  await recordCamera(page);
  await slowFix(page, 2000);
  await page.goto("/ride-now.html");
  const moves = (await settledCamera(page)).filter((c) => c.when === "moveend");

  expect(moves, "the network is not framed on the way to the fix").toHaveLength(1);
  expect(near(moves[0].lat, fix.latitude) && near(moves[0].lon, fix.longitude)).toBe(true);
  expect(moves[0].zoom).toBe(15.5);
});

test.describe("no location shared", () => {
  test.use({ permissions: [] });

  test("the next load opens on the frame the network was given, and does not move", async ({ page }) => {
    await quiet(page);
    await fallbackStyle(page);
    await recordCamera(page);
    await page.goto("/ride-now.html");
    await expect(page.getByTestId("ride-map")).not.toHaveAttribute("data-stops", "0", { timeout: 10_000 });
    await page.waitForTimeout(800);
    const framed = (await page.evaluate(() => window.__camera)).find((c) => c.when === "moveend");
    expect(framed, "the first load frames the network").toBeTruthy();

    await page.reload();
    await expect(page.getByTestId("ride-map")).not.toHaveAttribute("data-stops", "0", { timeout: 10_000 });
    await page.waitForTimeout(800);
    const camera = await page.evaluate(() => window.__camera);
    expect(camera[0].when).toBe("open");
    expect(near(camera[0].lat, framed.lat) && near(camera[0].lon, framed.lon), "opens where it was framed").toBe(true);
    expect(Math.abs(camera[0].zoom - framed.zoom)).toBeLessThan(0.01);
    expect(camera.filter((c) => c.when === "moveend"), "and the network is not framed again").toEqual([]);
  });

  test("the network is framed, once", async ({ page }) => {
    await quiet(page);
    await fallbackStyle(page);
    await recordCamera(page);
    await page.goto("/ride-now.html");
    await expect(page.getByTestId("ride-map")).not.toHaveAttribute("data-stops", "0", { timeout: 10_000 });
    await page.waitForTimeout(800);
    const moves = (await page.evaluate(() => window.__camera)).filter((c) => c.when === "moveend");

    expect(moves).toHaveLength(1);
    // The whole network in view — wider than the rider's zoom.
    expect(moves[0].zoom).toBeLessThan(15);
  });
});

test.describe("the tile archive", () => {
  test.skip(!fs.existsSync(path.join(webDir, "public/basemap.pmtiles")), "no local basemap.pmtiles (just basemap)");

  test("its header is fetched once, with no probe before it", async ({ page }) => {
    await quiet(page);
    const asked = [];
    page.on("request", (r) => {
      if (new URL(r.url()).pathname.endsWith("/basemap.pmtiles")) asked.push(`${r.method()} ${r.headers()["range"] ?? ""}`.trim());
    });
    await page.goto("/ride-now.html");
    await page.waitForFunction(() => window.__map?.loaded());

    expect(asked.filter((a) => a.startsWith("HEAD"))).toEqual([]);
    expect(asked.filter((a) => a === "GET bytes=0-16383"), "the archive's header").toHaveLength(1);
  });
});
