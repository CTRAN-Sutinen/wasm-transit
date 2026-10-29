// The published network (docs/ARCHITECTURE.md, server section): a
// server started with -feed imports its zip once and serves the
// network at /network; every generated UI boots from it with no GTFS
// in the browser, and refuses a network that does not register under
// the hash the server pins.
//
// Its own server (playwright.config.mjs, :8083) publishes the fixture
// feed — the shared e2e server publishes none, the file-input path
// the UI specs walk.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

test.use({
  baseURL: "http://127.0.0.1:8083",
  timezoneId: "UTC",
  geolocation: { latitude: 45.515, longitude: -73.515 },
  permissions: ["geolocation"],
});

async function live(page) {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00Z"));
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: "[]" }));
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
}

/// What the page asked the server for, by path.
function requests(page) {
  const paths = [];
  page.on("request", (r) => paths.push(new URL(r.url()).pathname));
  return paths;
}

test("ride now boots from the published network", async ({ page }) => {
  await live(page);
  const asked = requests(page);
  await page.goto("/ride-now.html");

  const first = page.getByTestId("nearby").getByTestId("stop-card").first();
  await expect(first).toContainText("Main Station", { timeout: 10_000 });
  await expect(first).toContainText("8:15");
  expect(asked).toContain("/network");
});

test("the stop display boots from the published network", async ({ page }) => {
  await live(page);
  await page.goto("/stop-display.html?stop=A&layout=routes");

  await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 10_000 });
  const ten = page.locator('[data-testid="display-group"][data-route="10"]').first();
  await expect(ten.getByTestId("display-time").first()).toContainText("2 min late");
});

test("service design opens the published network as a plan", async ({ page }) => {
  await page.goto("/service-design.html");

  await expect(page.getByTestId("type-table").locator("tbody tr")).toHaveCount(3, { timeout: 10_000 });
  await expect(page.getByTestId("plan-bar")).not.toContainText("the published network");
});

test("a network that does not register under the pinned hash is refused", async ({ page }) => {
  await live(page);
  await page.route("**/network", async (route) => {
    const served = await (await route.fetch()).json();
    served.network_hash = "sha256:" + "0".repeat(64);
    await route.fulfill({ contentType: "application/json", body: JSON.stringify(served) });
  });
  await page.goto("/ride-now.html");

  await expect(page.getByTestId("ride-status")).toContainText("the served network registers as", { timeout: 10_000 });
  await expect(page.getByTestId("nearby").getByTestId("stop-card")).toHaveCount(0);
});
