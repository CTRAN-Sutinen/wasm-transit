// Accessibility gate (docs/TESTING.md, "Accessibility"). Colors differ between
// the light and dark runs, and a phone shows the panel as a sheet.
// Service design joins when scope 3 of docs/units/web-conformance.md closes.

import { test, expect } from "@playwright/test";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const axeSource = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");
const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"];

const SETTINGS = [
  { name: "desktop light", viewport: { width: 1280, height: 800 }, colorScheme: "light" },
  { name: "phone dark", viewport: { width: 390, height: 844 }, colorScheme: "dark" },
];

test.use({
  // axe is injected as an inline script, which the rider pages' policy
  // refuses. This gate audits the DOM; e2e/headers.spec.mjs runs the
  // same pages under their policy.
  bypassCSP: true,
  baseURL: "http://127.0.0.1:8083",
  timezoneId: "UTC",
  geolocation: { latitude: 45.515, longitude: -73.515 },
  permissions: ["geolocation"],
});
test.setTimeout(90_000);

async function audit(page, label) {
  await page.addScriptTag({ path: axeSource });
  const result = await page.evaluate((tags) => window.axe.run(document, { runOnly: { type: "tag", values: tags } }), TAGS);
  const found = result.violations.map(
    (v) => `${v.id} (${v.impact}): ${v.nodes.map((n) => `${n.target.join(" ")} ${n.html.slice(0, 90)}`).slice(0, 4).join(" | ")}`,
  );
  expect.soft(found, label).toEqual([]);
}

async function feeds(page) {
  const trips = fs.readFileSync(path.join(fixtures, "sample-rt.pb"));
  const alerts = alertFeed();
  const body = (url) => (/alerts/i.test(url) ? alerts : trips);
  await page.route(/\/rt\?url=/, (route) => route.fulfill({ contentType: "application/octet-stream", body: body(route.request().url()) }));
  await page.route("http://feeds.test/**", (route) =>
    route.fulfill({ contentType: "application/octet-stream", headers: { "Access-Control-Allow-Origin": "*" }, body: body(route.request().url()) }),
  );
}

/// A GTFS-rt feed with one service alert on route 10 (the sample
/// fixture has none, and the alerts views need one). Protobuf by hand:
/// a length-delimited field is its tag, a length, and the bytes.
function alertFeed() {
  const field = (n, bytes) => Buffer.concat([Buffer.from([(n << 3) | 2, bytes.length]), bytes]);
  const text = (n, s) => field(n, Buffer.from(s));
  const translated = (s) => field(1, Buffer.concat([text(1, s), text(2, "en")]));
  const alert = Buffer.concat([
    field(5, text(2, "10")), // informed_entity { route_id }
    field(10, translated("Route 10 detour on Main Street")), // header_text
    field(11, translated("Buses skip 3rd Street until noon. Use the stop on 2nd.")), // description_text
  ]);
  return Buffer.concat([
    field(1, text(1, "2.0")), // header { gtfs_realtime_version }
    field(2, Buffer.concat([text(1, "alert-1"), field(5, alert)])), // entity { id, alert }
  ]);
}

async function setting(page, s) {
  await page.setViewportSize(s.viewport);
  await page.emulateMedia({ colorScheme: s.colorScheme });
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await feeds(page);
}

async function rideViews(page, label) {
  await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toContainText("Main Station", { timeout: 10_000 });
  await audit(page, `${label}: home`);

  await page.getByTestId("live-feeds").getByRole("button").first().click();
  await expect(page.getByRole("dialog", { name: "Live feeds" })).toBeVisible();
  await audit(page, `${label}: live feeds`);
  await page.keyboard.press("Escape");

  const input = page.getByLabel("stop name", { exact: true });
  await input.fill("main 1s");
  await expect(page.getByRole("option").first()).toContainText("Main & 1st");
  await audit(page, `${label}: search results`);
  await input.press("Enter");
  await expect(page.getByTestId("rider-board").locator("h2")).toHaveText("Main & 1st");
  await expect(page.getByTestId("board-rows").locator("li").first()).toBeVisible();
  await audit(page, `${label}: stop`);

  await page.getByRole("button", { name: "Directions from here" }).click();
  await expect(page.getByTestId("trip-plan")).toBeVisible();
  await audit(page, `${label}: trip, no destination`);
  const to = page.getByLabel("To", { exact: true });
  await to.fill("main 2");
  await expect(page.getByRole("option").first()).toContainText("Main & 2nd");
  await to.press("Enter");
  await expect(page.getByTestId("itinerary")).toBeVisible({ timeout: 10_000 });
  await audit(page, `${label}: trip`);

  await page.getByRole("button", { name: "back" }).click();
  await page.getByRole("button", { name: "back" }).click();
  await page.getByRole("button", { name: "Routes" }).click();
  await expect(page.getByTestId("route-list")).toBeVisible();
  await expect(page.getByTestId("route-list").locator("li").first()).toBeVisible();
  await audit(page, `${label}: routes`);
  await page.getByTestId("route-list").locator("li button").first().click();
  await expect(page.getByTestId("route-variant").first()).toBeVisible();
  await audit(page, `${label}: route`);

  await page.getByTestId("alerts-button").click();
  await expect(page.getByRole("heading", { name: "Service alerts" })).toBeVisible();
  await expect(page.getByTestId("alerts").first()).toContainText("Route 10 detour");
  await audit(page, `${label}: alerts`);

  await page.getByRole("button", { name: "back" }).click();
  await input.fill("main 1s");
  await expect(page.getByRole("option").first()).toContainText("Main & 1st");
  await input.press("Enter");
  await expect(page.getByTestId("board-rows").locator("li").first()).toBeVisible();
  await audit(page, `${label}: stop with an alert`);
  await page.getByTestId("board-rows").locator("li button").first().click();
  await page.getByRole("button", { name: /Track this bus/ }).click();
  await expect(page.getByTestId("bus-view")).toBeVisible();
  await audit(page, `${label}: bus`);
}

async function displayViews(page, label, prefix = "") {
  await page.goto(`${prefix}/stop-display.html`);
  await expect(page.getByTestId("display-setup")).toBeVisible({ timeout: 10_000 });
  await audit(page, `${label}: display setup`);
  for (const [layout, stop] of [["routes", "A"], ["center", "STN"], ["route", "C"]]) {
    await page.goto(`${prefix}/stop-display.html?stop=${stop}&layout=${layout}`);
    await expect(page.getByTestId("display-stop")).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId("display-group").first()).toBeVisible();
    await audit(page, `${label}: display ${layout}`);
  }
}

for (const s of SETTINGS) {
  test(`picker: ${s.name}`, async ({ page }) => {
    await setting(page, s);
    await page.goto("/");
    await expect(page.locator("h1").first()).toBeVisible();
    await audit(page, `picker, ${s.name}`);
  });

  test(`ride now: every view, ${s.name}`, async ({ page }) => {
    await setting(page, s);
    await page.goto("/ride-now.html");
    await rideViews(page, `ride now, ${s.name}`);
  });

  test(`stop display: every layout, ${s.name}`, async ({ page }) => {
    await setting(page, s);
    await displayViews(page, `stop display, ${s.name}`);
  });

  test.describe("static export", () => {
    test.use({ baseURL: "http://127.0.0.1:8087" });

    test(`ride now: every view, ${s.name}`, async ({ page }) => {
      await setting(page, s);
      await page.goto("/ride/ride-now.html");
      await rideViews(page, `static ride now, ${s.name}`);
    });

    test(`stop display: every layout, ${s.name}`, async ({ page }) => {
      await setting(page, s);
      await displayViews(page, `static stop display, ${s.name}`, "/display");
    });
  });
}
