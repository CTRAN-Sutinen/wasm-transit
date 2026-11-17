// Keyboard, focus, and preference gates for ride now
// (docs/units/web-conformance.md, scope 1). axe cannot see these:
//
//   1. A view change moves focus to the new view's heading and the
//      polite region speaks (WCAG 2.4.3, 4.1.3).
//   2. The page has one <main> landmark: the panel.
//   3. The "when" radio group is one Tab stop and takes the arrow keys
//      (the ARIA radio group pattern).
//   4. Text on the accent fill is the --on-accent token in both modes
//      (white on the dark accent is 2.26:1).
//   5. Motion is opt-in: the live dot pulses only for a rider who has
//      not asked for reduced motion.
//   6. A feed URL that fails is not "no network": the page says its
//      schedule could not be read, in a status region, and tries again.
//   7. The live-feeds panel acts as a dialog (D-135, no Popover API):
//      opening moves focus into it, Escape closes it and returns focus
//      to the pill, and a press outside it closes it.

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

async function open(page) {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
  await page.goto("/ride-now.html");
  await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toContainText("Main Station", { timeout: 10_000 });
}

test("a view change moves focus to the new heading and is announced", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("main")).toHaveCount(1);
  // Home has settled: the announcement counts the stops near the rider.
  await expect(page.getByTestId("ride-announce")).toContainText(/\d+ stops? near you/);

  await page.getByRole("button", { name: "Get directions" }).click();
  await expect(page.getByRole("heading", { name: "Get there" })).toBeFocused();

  await page.getByRole("button", { name: "back" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toBeFocused();

  await page.getByRole("button", { name: "Routes" }).click();
  await expect(page.getByRole("heading", { name: "Routes" })).toBeFocused();
  await expect(page.getByTestId("ride-announce")).toContainText(/\d+ routes?/);

  await page.getByTestId("route-list").locator("li button").first().click();
  await expect(page.getByTestId("route-view").getByRole("heading")).toBeFocused();
});

test("the when radio group is one tab stop and takes the arrow keys", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Get directions" }).click();
  const group = page.getByRole("radiogroup", { name: "when" });
  const radios = group.getByRole("radio");
  await expect(radios).toHaveCount(4);
  // Only the checked radio is in the tab order.
  await expect(group.locator('[role="radio"][tabindex="0"]')).toHaveCount(1);
  await expect(radios.nth(0)).toHaveAttribute("aria-checked", "true");

  await radios.nth(0).focus();
  await page.keyboard.press("ArrowRight");
  await expect(radios.nth(1)).toBeFocused();
  await expect(radios.nth(1)).toHaveAttribute("aria-checked", "true");
  await expect(radios.nth(0)).toHaveAttribute("aria-checked", "false");
  await page.keyboard.press("End");
  await expect(radios.nth(3)).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(radios.nth(0)).toBeFocused();
  await page.keyboard.press("ArrowLeft");
  await expect(radios.nth(3)).toBeFocused();
});

test("text on the accent fill takes the on-accent token in both modes", async ({ page }) => {
  await open(page);
  const cta = page.getByRole("button", { name: "Get directions" });
  const colors = async () =>
    cta.evaluate((el) => {
      const s = getComputedStyle(el);
      return { text: s.color, fill: s.backgroundColor };
    });
  await page.emulateMedia({ colorScheme: "light" });
  expect(await colors()).toEqual({ text: "rgb(255, 255, 255)", fill: "rgb(194, 65, 12)" });
  await page.emulateMedia({ colorScheme: "dark" });
  expect(await colors()).toEqual({ text: "rgb(28, 25, 23)", fill: "rgb(251, 146, 60)" });
});

test("the live dot pulses only without a reduced-motion preference", async ({ page }) => {
  await open(page);
  const dot = page.getByTestId("live-feeds").locator(".dot");
  await expect(page.getByTestId("live-feeds")).toContainText("Live");
  await page.emulateMedia({ reducedMotion: "no-preference" });
  expect(await dot.evaluate((el) => getComputedStyle(el).animationName)).not.toBe("none");
  await page.emulateMedia({ reducedMotion: "reduce" });
  expect(await dot.evaluate((el) => getComputedStyle(el).animationName)).toBe("none");
});

test("the live-feeds panel closes on Escape and on an outside press, and keeps focus", async ({ page }) => {
  await open(page);
  const pill = page.getByTestId("live-feeds").getByRole("button").first();
  const panel = page.getByRole("dialog", { name: "Live feeds" });

  await pill.click();
  await expect(panel).toBeVisible();
  await expect(page.getByLabel("trip updates url")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(panel).toBeHidden();
  await expect(pill).toBeFocused();

  // Escape inside the panel works from any control in it.
  await pill.click();
  await page.getByRole("button", { name: "Close" }).focus();
  await page.keyboard.press("Escape");
  await expect(panel).toBeHidden();
  await expect(pill).toBeFocused();

  // A press outside it closes it, and the press keeps its own target.
  await pill.click();
  await expect(panel).toBeVisible();
  await page.getByLabel("stop name", { exact: true }).click();
  await expect(panel).toBeHidden();
  await expect(page.getByLabel("stop name", { exact: true })).toBeFocused();

  // A press inside it does not.
  await pill.click();
  await page.getByLabel("service alerts url").click();
  await expect(panel).toBeVisible();
});

test.describe("a feed URL that fails", () => {
  test.use({ baseURL: "http://127.0.0.1:8088" });

  for (const [ui, url] of [
    ["ride now", "/ride/ride-now.html"],
    ["stop display", "/display/stop-display.html?stop=A&layout=routes"],
  ]) {
    test(`${ui} says the schedule could not be read, and tries again`, async ({ page }) => {
      // The agency's URL answers with an error first, then with the feed.
      let down = true;
      await page.route("http://localhost:8088/feeds/feed.zip", (route) =>
        down
          ? route.fulfill({ status: 503, body: "down" })
          : route.fulfill({
              contentType: "application/zip",
              headers: { "Access-Control-Allow-Origin": "*" },
              body: fs.readFileSync(path.join(fixtures, "sample-gtfs.zip")),
            }),
      );
      await page.clock.install({ time: new Date("2026-10-23T07:55:00-04:00") });
      await page.goto(url);
      const message = page.getByRole("status").filter({ hasText: "could not be read" });
      await expect(message.first()).toBeVisible({ timeout: 10_000 });
      await expect(page.getByText("publishes no network")).toHaveCount(0);

      // The next poll reads the URL again.
      down = false;
      await page.clock.fastForward(60_000);
      await expect(message).toHaveCount(0, { timeout: 15_000 });
    });
  }
});
