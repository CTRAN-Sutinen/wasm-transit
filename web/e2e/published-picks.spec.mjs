// Publish → serve → resolve (docs/ROADMAP.md 1): a pick published to
// the store (POST /plans) is the network a rider page loads once its
// date arrives. The page reads the manifest's plans_hash, asks the
// plans module which pick is in force today, and registers that pick's
// network by hash (GET /blobs/<hash>, gzipped) — before the first pick
// takes effect it reads the server's import as before.
//
// Its own server (playwright.config.mjs, :8084) runs the durable store
// in a fresh directory on a pinned date (2026-08-01) and publishes the
// fixture feed's import (its calendar runs through 2026-12-31); this
// spec publishes the picks.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

test.use({
  baseURL: "http://127.0.0.1:8084",
  timezoneId: "UTC",
  geolocation: { latitude: 45.515, longitude: -73.515 },
  permissions: ["geolocation"],
});

async function live(page, at) {
  await page.clock.setFixedTime(new Date(at));
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: "[]" }));
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
}

function requests(page) {
  const paths = [];
  page.on("request", (r) => paths.push(new URL(r.url()).pathname));
  return paths;
}

/// A pick: the published import, renamed and framed (a planner's fork).
async function publish(request, id, name, effectiveFrom) {
  const { network } = await (await request.get("/network")).json();
  const pick = { ...network, name, plan: { id, name, effective_from: effectiveFrom } };
  const resp = await request.post("/plans", { data: JSON.stringify(pick, null, 2) });
  expect(resp.status(), await resp.text()).toBe(200);
  return resp.json();
}

test("ride now loads the pick in force today", async ({ page, request }) => {
  const imported = (await (await request.get("/network")).json()).network.name;
  await publish(request, "fall-2026", "Fall 2026 pick", "2026-10-01");
  const winter = await publish(request, "winter-2026", "Winter 2026 pick", "2026-12-01");
  expect((await (await request.get("/manifest")).json()).plans_hash).toBe(winter.plans_hash);

  // 2026-10-23: the fall pick governs.
  await live(page, "2026-10-23T07:55:00-04:00");
  const asked = requests(page);
  await page.goto("/ride-now.html");
  await expect(page.locator("h1")).toHaveText("Fall 2026 pick", { timeout: 10_000 });
  expect(asked.some((p) => p.startsWith("/blobs/"))).toBe(true);
  expect(asked).not.toContain("/network/document");
  await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toContainText("Main Station");

  // Before the first pick takes effect: the import, as before.
  const early = await page.context().newPage();
  await live(early, "2026-09-01T07:55:00-04:00");
  await early.goto("/ride-now.html");
  await expect(early.locator("h1")).toHaveText(imported, { timeout: 10_000 });
});

test("the stop display loads the pick in force today", async ({ page, request }) => {
  await publish(request, "fall-2026", "Fall 2026 pick", "2026-10-01");
  await publish(request, "winter-2026", "Winter 2026 pick", "2026-12-01");
  await live(page, "2026-12-02T07:55:00-05:00");
  const asked = requests(page);
  await page.goto("/stop-display.html?stop=STN");
  await expect.poll(() => asked.some((p) => p.startsWith("/blobs/")), { timeout: 10_000 }).toBe(true);
  expect(asked).not.toContain("/network/document");
});

test("a retired pick never reaches a rider", async ({ page, request }) => {
  await publish(request, "fall-2026", "Fall 2026 pick", "2026-10-01");
  await publish(request, "winter-2026", "Winter 2026 pick", "2026-12-01");
  const retired = await request.post("/plans/retire", { data: { id: "winter-2026" } });
  expect(retired.status(), await retired.text()).toBe(200);
  await live(page, "2026-12-02T07:55:00-05:00");
  await page.goto("/ride-now.html");
  await expect(page.locator("h1")).toHaveText("Fall 2026 pick", { timeout: 10_000 });
  // Republishing restores it (the next spec run starts from a fresh store).
  await publish(request, "winter-2026", "Winter 2026 pick", "2026-12-01");
});

// The planner publishes from the service-design UI: the "Ready to
// publish?" panel posts the active plan to POST /plans and shows the
// server's answer as given — published, or each reason it was refused.
test("a planner publishes a fork from service design", async ({ page, request }) => {
  await page.goto("/service-design.html");
  await page.getByTestId("design-rail").getByRole("button", { name: "Plans" }).click();
  // The server's import is the base (its combinations, D-55: WK and
  // SUN); fork it as a pick.
  await expect(page.getByTestId("group-table").locator("tbody tr")).toHaveCount(2, { timeout: 10_000 });
  await page.getByTestId("fork-name").fill("Holiday 2026");
  await page.getByTestId("fork-date").fill("2026-11-23");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText("Forked");

  await page.keyboard.press("6");
  await page.getByTestId("publish").click();
  await expect(page.getByTestId("published")).toContainText("Published", { timeout: 10_000 });
  const { plans_hash } = await (await request.get("/manifest")).json();
  const set = await (await request.get(`/blobs/${plans_hash}`)).json();
  expect(set.plans.map((p) => p.name)).toContain("Holiday 2026");

  // A fork dated before the server's today is refused, with its reason.
  await page.keyboard.press("1");
  await page.getByTestId("fork-name").fill("Too late");
  await page.getByTestId("fork-date").fill("2026-07-01");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText("Forked");
  await page.keyboard.press("6");
  await page.getByTestId("publish").click();
  await expect(page.getByTestId("publish-refused")).toContainText("pick dated in past", { timeout: 10_000 });
});
