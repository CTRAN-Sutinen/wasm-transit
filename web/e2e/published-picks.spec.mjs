// Publish → serve → resolve (docs/ROADMAP.md 1): a pick published to
// the store (POST /plans) is the network a rider page loads once its
// date arrives. The page reads the manifest's plans_hash, asks the
// plans module which pick is in force today, and registers that pick's
// network by hash (GET /blobs/<hash>.gz) — before the first pick
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
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: '{"kind":"edit_records","schema_version":2,"records":[]}' }));
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
  expect(asked, "never the import's network").not.toContain(`/blobs/${(await (await page.request.get("/manifest")).json()).network_hash}.gz`);
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
  expect(asked, "never the import's network").not.toContain(`/blobs/${(await (await page.request.get("/manifest")).json()).network_hash}.gz`);
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

// A stop closed in the stops document (POST /stops, D-153) reaches a
// rider through the pick it touches (D-156: the server refreshes and
// republishes it): stop C — served by the A–C–B pattern — closed for
// Wednesday 2026-11-04 shows no departures that day, and its
// departures are back on Thursday. The other stops are as the import
// has them.
test("a stop closed for a day shows no departures that day", async ({ page, request }) => {
  await publish(request, "fall-2026", "Fall 2026 pick", "2026-10-01");
  await publish(request, "winter-2026", "Winter 2026 pick", "2026-12-01");
  const { network } = await (await request.get("/network")).json();
  const stops = {};
  for (const [id, s] of Object.entries(network.stops)) {
    stops[id] = [{
      event: "opened", lat: s.lat, lon: s.lon,
      ...(s.name ? { name: s.name } : {}), ...(s.kind === "station" ? { kind: "station" } : {}),
      ...(s.parent ? { parent: s.parent } : {}), ...(s.info ?? {}), ...(s.transfer ? { transfer: true } : {}),
    }];
  }
  stops.C.push({ on: "2026-11-04", event: "closed", until: "2026-11-05", note: "water main" });
  const from = (await (await request.get("/manifest")).json()).stops_hash ?? "";
  const written = await request.post("/stops", { data: { from, stops: { kind: "stops", schema_version: 1, stops } } });
  expect(written.status(), await written.text()).toBe(200);
  expect((await written.json()).republished).toContain("fall-2026");

  await live(page, "2026-11-04T07:55:00-05:00");
  await page.goto("/stop-display.html?stop=C&layout=routes");
  await expect(page.getByTestId("display-stop")).toHaveText(/./, { timeout: 10_000 });
  await expect(page.getByTestId("display-time")).toHaveCount(0);

  const next = await page.context().newPage();
  await live(next, "2026-11-05T07:55:00-05:00");
  await next.goto("/stop-display.html?stop=C&layout=routes");
  await expect(next.getByTestId("display-time").first()).toBeVisible({ timeout: 10_000 });
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

// The Stops stage (service-change) reads the stops document on the date
// the planner chooses and writes a closure through POST /stops (D-144,
// D-153, D-155): stop B closed from Monday 2026-11-09, open again from
// Wednesday 11-11, reads closed that week on its card, on the map, and
// as a marked column of route 10's timetable (its pick, Fall 2026, is
// republished with the closure) — and open again on the Wednesday.
test("a planner closes a stop for dates on the Stops stage", async ({ page, request }) => {
  await publish(request, "fall-2026", "Fall 2026 pick", "2026-10-01");
  await publish(request, "winter-2026", "Winter 2026 pick", "2026-12-01");
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/service-change.html#stage=stops&open=B&on=2026-11-10");
  await expect(page.getByTestId("plan-select")).toHaveValue("published:fall-2026", { timeout: 15_000 });
  await expect(page.getByTestId("stops-on")).toHaveValue("2026-11-10");
  await expect(page.getByTestId("stop-events")).toBeVisible({ timeout: 10_000 });

  await page.getByTestId("stop-edit-from").fill("2026-11-09");
  await page.getByTestId("stop-edit-until").fill("2026-11-11");
  await page.getByTestId("stop-edit-note").fill("Paving");
  await page.getByTestId("stop-edit-submit").click();
  await expect(page.getByTestId("status")).toContainText("Closed B from 2026-11-09 until 2026-11-11", { timeout: 15_000 });
  await expect(page.getByTestId("status")).toContainText("Fall 2026 pick");
  await expect(page.getByTestId("stop-status")).toContainText("Closed");
  await expect(page.getByTestId("stop-event").last()).toContainText("Paving");
  await expect(page.getByTestId("stop-map-closed")).toBeVisible();
  // An A–B trip has one stop left that week: it does not run (a warning).
  await expect(page.getByTestId("stop-warnings")).toContainText("fewer than two open stops from 2026-11-09 until 2026-11-11");

  // The timetable marks B's column that day.
  await page.getByTestId("stop-visit").first().getByRole("button").click();
  await expect(page.getByTestId("stage-route")).toBeVisible();
  await page.getByLabel("timetable").check();
  const marked = page.getByTestId("timetable").locator("th[data-unserved]");
  await expect(marked).toHaveCount(1, { timeout: 10_000 });
  await expect(marked).toHaveAttribute("data-unserved", "closed");
  await expect(marked).toContainText("closed");

  // Wednesday: open again, nothing marked.
  await page.getByTestId("stops-on").fill("2026-11-11");
  await expect(marked).toHaveCount(0, { timeout: 10_000 });
  await page.getByRole("navigation", { name: "Stages" }).getByRole("button", { name: /Stops/ }).click();
  await expect(page.getByTestId("stop-status")).toContainText("Open", { timeout: 10_000 });
});
