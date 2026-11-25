// The ride map opens on the view the rider ends on, so the camera makes no
// move whose tiles are thrown away, and it asks for its tile archive once
// (src/viewport.ts, src/basemap.ts). The first three tests run on the fallback
// style. The archive test needs the local basemap.pmtiles and skips without it.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const webDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
// Main Station. The center of stops A..C is about 1.4 km away.
const fix = { latitude: 45.515, longitude: -73.515 };
const near = (a, b) => Math.abs(a - b) < 0.002;

// The server on :8083 publishes the fixture network, so the map has stops to frame.
test.use({ baseURL: "http://127.0.0.1:8083", timezoneId: "UTC", geolocation: fix, permissions: ["geolocation"] });

async function quiet(page) {
  await page.route(/\/rt\?url=/, (route) => route.fulfill({ status: 404 }));
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: '{"kind":"edit_records","schema_version":2,"records":[]}' }));
}

/// OpenFreeMap's style is empty here, so the map loads offline.
async function fallbackStyle(page) {
  await page.route("**/basemap.pmtiles", (route) => route.fulfill({ status: 404 }));
  await page.route("https://tiles.openfreemap.org/**", (route) =>
    route.fulfill({ contentType: "application/json", body: JSON.stringify({ version: 8, sources: {}, layers: [] }) }),
  );
}

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

async function settledCamera(page) {
  await expect(page.getByTestId("ride-map")).not.toHaveAttribute("data-stops", "0", { timeout: 10_000 });
  await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toBeVisible({ timeout: 10_000 });
  await page.waitForTimeout(800);
  return page.evaluate(() => window.__camera);
}

async function slowFix(page, ms) {
  await page.addInitScript((ms) => {
    const ask = navigator.geolocation.getCurrentPosition.bind(navigator.geolocation);
    navigator.geolocation.getCurrentPosition = (ok, err, opts) => setTimeout(() => ask(ok, err, opts), ms);
  }, ms);
}

test("a rider who shared a location before opens on it and the camera never leaves", async ({ page }) => {
  await quiet(page);
  await fallbackStyle(page);
  await page.goto("/ride-now.html");
  await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toBeVisible({ timeout: 10_000 });

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
