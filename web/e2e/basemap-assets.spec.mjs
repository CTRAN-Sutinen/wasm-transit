// The basemap style reads glyphs and sprites from `basemap-assets/` when
// `just basemap-assets` installed them, else from protomaps.github.io (D-49).
// The spec stubs the assets and skips without a local basemap.pmtiles.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const webDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
test.skip(!fs.existsSync(path.join(webDir, "public/basemap.pmtiles")), "no local basemap.pmtiles (just basemap)");

test.use({
  geolocation: { latitude: 45.515, longitude: -73.515 },
  permissions: ["geolocation"],
});

async function quiet(page) {
  await page.route(/\/rt\?url=/, (route) => route.fulfill({ status: 404 }));
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: '{"kind":"edit_records","schema_version":2,"records":[]}' }));
}

async function styleUrls(page) {
  await page.goto("/ride-now.html");
  await page.waitForFunction(() => window.__map?.getStyle()?.glyphs);
  return page.evaluate(() => {
    const s = window.__map.getStyle();
    return { glyphs: s.glyphs, sprite: s.sprite };
  });
}

test("with the assets installed the style reads them from the app's own origin", async ({ page, baseURL }) => {
  await quiet(page);
  const outside = [];
  page.on("request", (r) => {
    if (/protomaps\.github\.io/.test(r.url())) outside.push(r.url());
  });
  await page.route("**/basemap-assets/**", (route) =>
    route.request().url().endsWith(".json")
      ? route.fulfill({ contentType: "application/json", body: "{}" })
      : route.fulfill({ contentType: "application/octet-stream", body: "" }),
  );

  const { glyphs, sprite } = await styleUrls(page);
  expect(glyphs).toBe(`${baseURL}/basemap-assets/fonts/{fontstack}/{range}.pbf`);
  expect(sprite).toBe(`${baseURL}/basemap-assets/sprites/v4/light`);
  await page.waitForTimeout(500);
  expect(outside, "no label asset comes from the network").toEqual([]);
});

test("without them the style falls back to protomaps.github.io", async ({ page }) => {
  await quiet(page);
  // The static server's own answer for a file it does not hold; the
  // network fallback is stubbed so the spec needs no uplink.
  await page.route("**/basemap-assets/**", (route) => route.fulfill({ status: 404 }));
  await page.route(/protomaps\.github\.io/, (route) => route.fulfill({ status: 404 }));

  const { glyphs, sprite } = await styleUrls(page);
  expect(glyphs).toBe("https://protomaps.github.io/basemaps-assets/fonts/{fontstack}/{range}.pbf");
  expect(sprite).toBe("https://protomaps.github.io/basemaps-assets/sprites/v4/light");
});
