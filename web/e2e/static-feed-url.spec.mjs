// A fully serverless deployment that follows the agency's GTFS URL
// (D-132): the export names the feed (`-feed-url`) instead of
// importing it, and each page fetches the zip itself, imports it in its
// own assembly, and checks the URL again once a day — a changed zip is
// re-imported and adopted without a reload, an unchanged one is not.
// The service worker keeps the last zip, so a device boots offline on
// the schedule it last had.
//
// Its host (playwright.config.mjs, :8088) serves the stop display's
// bundle under /display/, ride-now's under /ride/, and the feed at
// /feeds/feed.zip with CORS; the manifest names
// the feed by another origin (localhost against 127.0.0.1), as an
// agency's feed host is.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import os from "node:os";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");
const feedFile = path.join(os.tmpdir(), "transit-e2e-static-fetch", "feeds", "feed.zip");
const FEED_URL = "http://localhost:8088/feeds/feed.zip";

test.use({
  baseURL: "http://127.0.0.1:8088",
  timezoneId: "UTC",
  geolocation: { latitude: 45.515, longitude: -73.515 },
  permissions: ["geolocation"],
});

async function feeds(page) {
  await page.route("http://feeds.test/**", (route) =>
    route.fulfill({
      contentType: "application/octet-stream",
      headers: { "Access-Control-Allow-Origin": "*" },
      body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")),
    }),
  );
}

function requests(page) {
  const urls = [];
  page.on("request", (r) => urls.push(r.url()));
  return urls;
}

const networks = async (page) =>
  (await page.evaluate(() => window.__bus.held("board"))).held.filter((e) => e.kind === "network").map((e) => e.key);

test("the stop display boots from the agency's GTFS URL", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await feeds(page);
  const asked = requests(page);
  await page.goto("/display/stop-display.html?stop=A&layout=routes");

  await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 15_000 });
  const ten = page.locator('[data-testid="display-group"][data-route="10"]').first();
  await expect(ten.getByTestId("display-time").first()).toContainText("2 min late");
  expect(asked).toContain(FEED_URL);
  expect(asked.filter((u) => u.includes("/blobs/")), "no network is published").toEqual([]);
});

test("ride now boots from the agency's GTFS URL", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await feeds(page);
  await page.goto("/ride/ride-now.html");
  const first = page.getByTestId("nearby").getByTestId("stop-card").first();
  await expect(first).toContainText("Main Station", { timeout: 15_000 });
  await expect(first).toContainText("8:15");
});

test("a changed feed is adopted at the next daily check, without a reload", async ({ page }) => {
  const original = fs.readFileSync(feedFile);
  try {
    await page.clock.install({ time: new Date("2026-10-23T07:55:00-04:00") });
    await feeds(page);
    const asked = requests(page);
    await page.goto("/display/stop-display.html?stop=A&layout=routes");
    await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 15_000 });
    await page.evaluate(() => (window.__notReloaded = true));
    const [held] = await networks(page);
    const fetches = () => asked.filter((u) => u === FEED_URL).length;
    const booted = fetches();

    // The agency publishes a new zip.
    fs.copyFileSync(path.join(fixtures, "overnight-gtfs.zip"), feedFile);

    // A poll within the day neither fetches nor re-pins.
    await page.clock.fastForward(20_000);
    await page.clock.fastForward(20_000);
    expect(fetches(), "the feed is checked once a day, not every poll").toBe(booted);
    expect(await networks(page)).toEqual([held]);

    // A day on: the next poll checks the URL and adopts the new network.
    await page.clock.fastForward(24 * 3600_000);
    await expect.poll(async () => (await networks(page)).length === 1 && (await networks(page))[0] !== held, { timeout: 15_000 }).toBe(true);
    expect(await page.evaluate(() => window.__notReloaded)).toBe(true);
  } finally {
    fs.writeFileSync(feedFile, original);
  }
});

test.describe("offline", () => {
  test.use({ serviceWorkers: "allow" });

  test("the stop display boots offline on the last zip it had", async ({ page, context }) => {
    await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
    await feeds(page);
    await page.goto("/display/stop-display.html?stop=A&layout=routes");
    await expect.poll(() => page.evaluate(() => !!navigator.serviceWorker?.controller), { timeout: 20_000 }).toBe(true);
    await page.reload();
    await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 15_000 });

    await context.setOffline(true);
    await page.reload();
    await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 15_000 });
    await expect(page.locator('[data-testid="display-group"][data-route="10"]').first()).toBeVisible();
    await context.setOffline(false);
  });
});
