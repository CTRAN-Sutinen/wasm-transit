// Publish → serve → resolve (docs/ROADMAP.md 1): a pick published to
// the store (POST /plans) is the network a rider page loads once its
// date arrives. The page reads the manifest's plans_hash, asks the
// plans module which pick is in force today, and registers that pick's
// network by hash (GET /blobs/<hash>, gzipped) — before the first pick
// takes effect it reads the server's import as before.
//
// Its own server (playwright.config.mjs, :8084) runs the durable store
// in a fresh directory and publishes the fixture feed's import; this
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
  const spring = await publish(request, "spring-2027", "Spring 2027 pick", "2027-03-01");
  expect((await (await request.get("/manifest")).json()).plans_hash).toBe(spring.plans_hash);

  // 2026-10-23: the fall pick governs.
  await live(page, "2026-10-23T07:55:00Z");
  const asked = requests(page);
  await page.goto("/ride-now.html");
  await expect(page.locator("h1")).toHaveText("Fall 2026 pick", { timeout: 10_000 });
  expect(asked.some((p) => p.startsWith("/blobs/"))).toBe(true);
  expect(asked).not.toContain("/network/document");
  await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toContainText("Main Station");

  // Before the first pick takes effect: the import, as before.
  const early = await page.context().newPage();
  await live(early, "2026-09-01T07:55:00Z");
  await early.goto("/ride-now.html");
  await expect(early.locator("h1")).toHaveText(imported, { timeout: 10_000 });
});

test("the stop display loads the pick in force today", async ({ page, request }) => {
  await publish(request, "fall-2026", "Fall 2026 pick", "2026-10-01");
  await publish(request, "spring-2027", "Spring 2027 pick", "2027-03-01");
  await live(page, "2027-03-02T07:55:00Z");
  const asked = requests(page);
  await page.goto("/stop-display.html?stop=STN");
  await expect.poll(() => asked.some((p) => p.startsWith("/blobs/")), { timeout: 10_000 }).toBe(true);
  expect(asked).not.toContain("/network/document");
});
