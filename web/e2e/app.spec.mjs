// End-to-end tests: real Chromium against the built app served by
// transit-server, exercising the flows a user actually performs.
//
// Fixture feed (sample-gtfs.zip, see make_fixtures.py):
//   t10: A 08:00 -> B 08:10 (shape sh1), t20: A 08:20 -> B 08:30
//   t30: A 08:40 -> C 08:45 -> B 08:50 (the interior-stop trip)
//   t11: A 09:00 -> B 09:10, SUN service only
//   t12: B 07:30, HOL service added on Friday 2026-10-23
//   all Friday service except where noted. Fixture realtime
//   (sample-rt.pb): t10 delayed +120 s at A / +180 s at B, one
//   vehicle at 45.44/-73.44 on t10.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

async function loadFeed(page) {
  // Deterministic clock: Friday 2026-10-23 at 07:30, so the fixture's
  // 08:00/08:20 trips are always upcoming regardless of real time.
  await page.evaluate(() => window.__setNow({ date: "2026-10-23", secs: 27000 }));
  // Scoped to the data loader: the planner panel carries its own
  // (hidden) file input for network documents.
  await page.locator(".dropzone input[type=file]").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  await expect(page.getByText(/Loaded|Restored/)).toBeVisible();
}

/// Switch the panel to a persona view (Rider / Dispatcher / Planner).
async function switchPersona(page, name) {
  await page.getByRole("tab", { name }).click();
}

/// Wait until the MapLibre instance exists and its sources are live.
async function waitForMap(page) {
  await page.waitForFunction(() => {
    const map = window.__map;
    return map && map.getSource("stops") && map.getSource("itinerary");
  });
}

/// Source data, normalized: some sources hold a FeatureCollection, the
/// selected-trip source holds a bare Feature.
async function mapData(page, source) {
  return page.evaluate((name) => {
    const data = window.__map.getSource(name).serialize().data;
    return data.type === "FeatureCollection" ? data : { features: [data] };
  }, source);
}

test("loads cleanly with no console errors", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(`pageerror: ${e}`));
  page.on("console", (m) => {
    if (m.type() === "error" && !m.text().includes("favicon")) {
      errors.push(m.text());
    }
  });
  await page.goto("/");
  await expect(page.getByText(/wasm ready/i)).toBeVisible();
  await waitForMap(page);
  expect(errors).toEqual([]);
});

test("loading a feed populates summary, board and map", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  // compact feed line (the persona split retired the summary table)
  await expect(page.locator(".feed-line")).toContainText("5 trips");

  // departures render for the first stop once selected
  await page.getByLabel("Stop").selectOption("A");
  const rows = page.locator("section:has(h2:text('Departures')) tbody tr");
  await expect(rows).toHaveCount(3, { timeout: 10_000 });

  // stops are drawn on the map
  await expect
    .poll(async () => (await mapData(page, "stops")).features.length)
    .toBe(3);
});

test("clicking a departure draws the trip path", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);
  await page.getByLabel("Stop").selectOption("A");
  const departures = page.locator("section", { has: page.getByRole("heading", { name: "Departures" }) });
  await departures.getByRole("row", { name: /08:00/ }).click();
  await expect
    .poll(async () => (await mapData(page, "trip")).features.length)
    .toBeGreaterThan(0);
});

test("trip planner draws the ridden segment, not the whole route", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  await page.getByLabel("From", { exact: true }).selectOption("A");
  await page.getByLabel("To", { exact: true }).selectOption("B");
  await page.locator("#plan-time").fill("07:30");
  await page.getByRole("button", { name: "Plan trip" }).click();

  // overlay shows the plan
  const overlay = page.locator(".plan-overlay");
  await expect(overlay).toBeVisible();
  await expect(overlay.getByText(/arrive/i)).toBeVisible();
  await expect(overlay.getByText("08:10", { exact: true })).toBeVisible();

  // the itinerary transit leg is the A->B segment of shape sh1:
  // 3 points (clipped), not the whole 5-point route
  await expect
    .poll(async () => {
      const data = await mapData(page, "itinerary");
      return data.features.map((f) => f.geometry.coordinates.length);
    })
    .toEqual([3]);
});

test("unplannable trip gives inline feedback", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  // B -> A has no service at any time (t10/t20 only run A -> B)
  await page.getByLabel("From", { exact: true }).selectOption("B");
  await page.getByLabel("To", { exact: true }).selectOption("A");
  await page.getByRole("button", { name: "Plan trip" }).click();

  await expect(page.locator(".plan-notice")).toContainText(/No trip found/i);
  const itinerary = await mapData(page, "itinerary");
  expect(itinerary.features.length).toBe(0);
});

test("refresh restores the feed from local cache", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  await page.reload();
  await expect(page.getByText(/Restored/)).toBeVisible({ timeout: 10_000 });
  await expect(page.locator(".feed-line")).toContainText("5 trips");
});

test("realtime feed colors delays and vehicles", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  await page.locator(".dropzone input[type=file]").setInputFiles(path.join(fixtures, "sample-rt.pb"));

  // severity chip for the delayed trip (worst +180 s -> "late 1-5 min")
  await expect(page.getByText(/late 1–5 min/)).toBeVisible();

  // the vehicle dot carries the joined delay
  await expect
    .poll(async () => (await mapData(page, "vehicles")).features[0]?.properties.delay)
    .toBe(180);

  // the departure board predicts the delayed time: 08:00 + 120 s
  await page.getByLabel("Stop").selectOption("A");
  await expect(page.getByText("08:02")).toBeVisible();
  await expect(page.getByText("+2 min")).toBeVisible();
});

/// Create a diversion through the editor UI: route 10, anchors
/// A -> B, interior stop C auto-skipped, `mins` minutes added.
async function makeDiversion(page, mins) {
  await page.getByRole("button", { name: "New diversion" }).click();
  await page.getByLabel("Route").selectOption("10");
  // Click the two anchors in the stop-sequence list (A, C, B).
  await page.locator(".stop-seq").getByRole("button", { name: "Main & 1st" }).click();
  await page.locator(".stop-seq").getByRole("button", { name: "Main & 2nd" }).click();
  await page.getByLabel("Added time (min)").fill(String(mins));
  await page.getByRole("button", { name: "Apply diversion" }).click();
  await expect(page.getByText(/Diversion applied: 3 trips/)).toBeVisible();
}

test("diversion reroutes the schedule, the board and the planner", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  // Route 10, anchors A -> B, skip C, +5 minutes — dispatcher work.
  await switchPersona(page, "Dispatcher");
  await makeDiversion(page, 5);

  // The rider's view: C loses its only service (t30 used to run A -> C -> B).
  await switchPersona(page, "Rider");
  await page.getByLabel("Stop").selectOption("C");
  const rows = page.locator("section:has(h2:text('Departures')) tbody tr");
  await expect(rows).toHaveCount(0);

  // B's board shifts: t10 08:15, t30 08:55 (t12/t20 untouched).
  await page.getByLabel("Stop").selectOption("B");
  await expect(page.getByText("08:15", { exact: true })).toBeVisible();
  await expect(page.getByText("08:55", { exact: true })).toBeVisible();
  await expect(page.getByText("08:10", { exact: true })).toHaveCount(0);

  // The planner rides the diverted schedule and shows the delta.
  await page.getByLabel("From", { exact: true }).selectOption("A");
  await page.getByLabel("To", { exact: true }).selectOption("B");
  await page.locator("#plan-time").fill("07:30");
  await page.getByRole("button", { name: "Plan trip" }).click();
  await expect(page.locator(".plan-overlay")).toBeVisible();
  await expect(page.locator(".plan-overlay").getByText("08:15", { exact: true })).toBeVisible();
  await expect(page.locator(".plan-compare")).toContainText(/without the active diversion: arrive 08:10\s+\(5 min later\)/i);

  // A -> C is unreachable while the diversion is active; the notice
  // says what the baseline looked like.
  await page.getByLabel("To", { exact: true }).selectOption("C");
  await page.getByRole("button", { name: "Plan trip" }).click();
  await expect(page.locator(".plan-notice")).toContainText(/without it you would arrive 08:45/);

  // Toggle the diversion off (dispatcher again): C's service returns.
  await switchPersona(page, "Dispatcher");
  await page.locator(".diversion-list input[type=checkbox]").first().uncheck();
  await switchPersona(page, "Rider");
  await page.getByLabel("Stop").selectOption("C");
  await expect(rows).toHaveCount(1);
  await expect(page.getByText("08:45", { exact: true })).toBeVisible();

  // The edit persists in IndexedDB next to the feed: after a reload
  // it is still listed (retired, as toggled — the tombstone survives).
  // The persona persists too: the panel comes back in Rider.
  await page.evaluate(() => window.__flushEdits());
  await page.reload();
  await waitForMap(page);
  await expect(page.getByText(/Restored/)).toBeVisible({ timeout: 10_000 });
  await switchPersona(page, "Dispatcher");
  await expect(page.locator(".diversion-list li.inactive")).toHaveCount(1);
});

test("diversion ripple shows as a tight block layover", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  // Blocks and diversions live in the dispatcher view.
  await switchPersona(page, "Dispatcher");

  // Block B1: t10 08:00 -> 08:10, then t20 at 08:20 (10 min layover).
  const blocks = page.locator("section", { has: page.getByRole("heading", { name: "Blocks" }) });
  await expect(blocks.getByText("10 min")).toBeVisible();
  await expect(blocks.getByText("tight")).toHaveCount(0);

  // Divert route 10 with 6 extra minutes: t10 now ends 08:16 and the
  // layover shrinks to 4 minutes — flagged.
  await makeDiversion(page, 6);
  await expect(blocks.getByText("tight")).toBeVisible();
  await expect(blocks.getByText("4 min")).toBeVisible();
});

test("street network import paginates ArcGIS-style queries", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);

  // The /rt proxy, mocked as an ArcGIS FeatureServer: page 1 is
  // flagged exceededTransferLimit, page 2 (resultOffset advanced)
  // ends the sequence. One street segment per page.
  let requests = 0;
  await page.route(/\/rt\?url=/, (route) => {
    requests += 1;
    const url = new URL(route.request().url());
    const target = decodeURIComponent(url.searchParams.get("url"));
    const offset = new URL(target).searchParams.get("resultOffset") ?? "0";
    const line = (lon, lat, dlon, dlat) => ({
      type: "Feature",
      properties: {},
      geometry: { type: "LineString", coordinates: [[lon, lat], [lon + dlon, lat + dlat]] },
    });
    return route.fulfill({
      json: Number(offset) === 0
        ? { type: "FeatureCollection", properties: { exceededTransferLimit: true }, features: [line(-73.5, 45.5, 0.01, 0.01)] }
        : { type: "FeatureCollection", properties: {}, features: [line(-73.6, 45.6, 0.01, 0.01)] },
    });
  });

  // No feed needed: streets are app config, loadable on their own.
  await page.fill("#streets-url", "https://portal.example/roads/FeatureServer/0/query?where=1%3D1&f=geojson");
  await page.getByRole("button", { name: "Import", exact: true }).click();

  await expect(page.getByText("2 street segments — snapping on")).toBeVisible();
  expect(requests).toBe(2);
  expect((await mapData(page, "streets")).features.length).toBe(2);
});

/// Wait until the camera has settled on the fixture feed's stops:
/// loading the feed fires a fitBounds whose flight (from the local
/// basemap's initial view) lasts seconds, and pixel-precise clicks
/// mid-flight land at meaningless lngLats.
async function settleOnStops(page) {
  await page.waitForFunction(
    () => {
      const map = window.__map;
      if (!map) return false;
      const c = map.getCenter();
      return (
        !map.isMoving() && c.lng > -73.55 && c.lng < -73.45 &&
        Math.abs(c.lat - 45.51) < 0.02 && map.getZoom() > 10
      );
    },
    null, { timeout: 30_000 },
  );
}

test("detour drawing snaps to an imported street network", async ({ page }) => {
  // Slow by nature: loading the feed flies the camera from the
  // basemap's initial view across the country to the stops.
  test.setTimeout(90_000);
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);
  await switchPersona(page, "Dispatcher");
  await settleOnStops(page);

  await page.getByRole("button", { name: "New diversion" }).click();

  // Selecting a route shows its alignment — the ghost the detour is
  // drawn against (t30 is route 10's representative: A → C → B).
  await page.getByLabel("Route").selectOption("10");
  const ghost = (await mapData(page, "route-context")).features;
  expect(ghost.length).toBe(1);
  expect(ghost[0].geometry.coordinates.length).toBe(3);

  // Import the street network: Side St (2 segments) + Nowhere Rd (1)
  // + Elbow Ln (2). The config panel auto-closed when the feed
  // loaded, but its inputs stay attached — the import works
  // regardless.
  await page.locator("#streets-file").setInputFiles(path.join(fixtures, "streets.geojson"));
  await expect(
    page.locator(".diversion-editor").getByText("snapping on"),
  ).toBeVisible();
  expect((await mapData(page, "streets")).features.length).toBe(3);

  await page.getByRole("button", { name: "Trace path on map" }).click();

  // Point-segment distance in degrees (planar is fine at this scale).
  const distToSeg = (lon, lat, a, b) => {
    const dx = b[0] - a[0], dy = b[1] - a[1];
    const len2 = dx * dx + dy * dy;
    const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((lon - a[0]) * dx + (lat - a[1]) * dy) / len2));
    return Math.hypot(lon - (a[0] + t * dx), lat - (a[1] + t * dy));
  };
  const sideSt = [
    [[-73.4990, 45.5030], [-73.5060, 45.5100]],
    [[-73.5060, 45.5100], [-73.5130, 45.5170]],
  ];

  // Click ~12 px east of Side St's midpoint: the drawn vertex must
  // land ON the street, not where the cursor was. Integer pixels —
  // mouse events dispatch at whole-pixel positions.
  const near = await page.evaluate(() => {
    const p = window.__map.project([-73.5060, 45.5100]);
    return { x: Math.round(p.x) + 10, y: Math.round(p.y) + 6 };
  });
  const rawNear = await page.evaluate(
    ({ x, y }) => window.__map.unproject([x, y]),
    near,
  );
  await page.mouse.click(near.x, near.y);

  // A click far from every street and stop keeps its raw position.
  const far = await page.evaluate(() => {
    const p = window.__map.project([-73.5060, 45.5100]);
    return { x: Math.round(p.x) - 200, y: Math.round(p.y) + 100 };
  });
  const rawFar = await page.evaluate(
    ({ x, y }) => window.__map.unproject([x, y]),
    far,
  );
  await page.mouse.click(far.x, far.y);

  const drawn = (await mapData(page, "detour-draft")).features.filter(
    (f) => f.geometry.type === "Point",
  );
  expect(drawn.length).toBe(2);

  const [snappedLon, snappedLat] = drawn[0].geometry.coordinates;
  const snappedDist = Math.min(...sideSt.map(([a, b]) => distToSeg(snappedLon, snappedLat, a, b)));
  expect(snappedDist).toBeLessThan(2e-6); // on the street geometry
  expect(Math.hypot(snappedLon - rawNear.lng, snappedLat - rawNear.lat))
    .toBeGreaterThan(1e-5); // not the raw click

  const [rawLon, rawLat] = drawn[1].geometry.coordinates;
  expect(rawLon).toBeCloseTo(rawFar.lng, 7);
  expect(rawLat).toBeCloseTo(rawFar.lat, 7);

  // A drag along the street places waypoints every ~30 px; each step
  // ROUTES along the graph, so the trace commits Side St's own
  // vertices — including v1 (-73.5060, 45.5100), crossed mid-stroke.
  const start = await page.evaluate(() => {
    const p = window.__map.project([-73.5025, 45.5065]); // mid first segment
    return { x: Math.round(p.x), y: Math.round(p.y) };
  });
  const end = await page.evaluate(() => {
    const p = window.__map.project([-73.5095, 45.5135]); // mid second segment
    return { x: Math.round(p.x), y: Math.round(p.y) };
  });
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  for (let i = 1; i <= 6; i++) {
    await page.mouse.move(
      Math.round(start.x + (end.x - start.x) * (i / 6)),
      Math.round(start.y + (end.y - start.y) * (i / 6)),
    );
  }
  await page.mouse.up();
  const stroked = (await mapData(page, "detour-draft")).features.filter(
    (f) => f.geometry.type === "Point",
  );
  expect(stroked.length).toBeGreaterThan(4); // 2 clicks + routed waypoints
  const v1 = stroked.some((f) =>
    Math.hypot(f.geometry.coordinates[0] + 73.5060, f.geometry.coordinates[1] - 45.5100) < 1e-6
  );
  expect(v1).toBe(true); // the trace follows street geometry, not chords
  // Undo reverts the last routed step (its whole geometry).
  const before = stroked.length;
  await page.getByRole("button", { name: "Undo" }).click();
  const undone = (await mapData(page, "detour-draft")).features.filter(
    (f) => f.geometry.type === "Point",
  );
  expect(undone.length).toBeLessThan(before);
});

test("detour trace routes along the street graph, not straight lines", async ({ page }) => {
  test.setTimeout(90_000);
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);
  await switchPersona(page, "Dispatcher");
  await settleOnStops(page);

  await page.getByRole("button", { name: "New diversion" }).click();
  await page.locator("#streets-file").setInputFiles(path.join(fixtures, "streets.geojson"));
  await page.getByRole("button", { name: "Trace path on map" }).click();

  // Two waypoints around Elbow Ln's corner: start (-73.5030, 45.5040)
  // → corner (-73.5080, 45.5040) → end (-73.5080, 45.5090). A
  // straight-line trace would cut the diagonal; the routed trace
  // must commit the corner vertex exactly.
  const clickAt = (lonLat) =>
    page.evaluate(([lon, lat]) => {
      const p = window.__map.project([lon, lat]);
      return { x: Math.round(p.x), y: Math.round(p.y) };
    }, lonLat);
  const e1 = await clickAt([-73.5030, 45.5040]);
  const e2 = await clickAt([-73.5080, 45.5080]);
  await page.mouse.click(e1.x, e1.y);
  await page.mouse.click(e2.x, e2.y);

  const drawn = (await mapData(page, "detour-draft")).features.filter(
    (f) => f.geometry.type === "Point",
  );
  expect(drawn.length).toBe(3); // start, corner, end — no diagonal
  // The routed path is street geometry: the corner, then the
  // endpoint (the exact snapped position of a whole-pixel click,
  // hence 1e-5 tolerance rather than exactness).
  expect(drawn[1].geometry.coordinates[0]).toBeCloseTo(-73.5080, 5);
  expect(drawn[1].geometry.coordinates[1]).toBeCloseTo(45.5040, 5);
  expect(drawn[2].geometry.coordinates[0]).toBeCloseTo(-73.5080, 5);
  expect(drawn[2].geometry.coordinates[1]).toBeCloseTo(45.5080, 5);
});

test("diversion syncs to the server and back", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);
  await switchPersona(page, "Dispatcher");

  // Push: create a diversion (a local draft), sync, and the server
  // document must hold it as submitted. Other tests in this run may
  // have left retired records on the server (every reload syncs), so
  // assert on *this* record, not on document shape.
  await makeDiversion(page, 5);
  await page.getByRole("button", { name: "Sync with server" }).click();
  await expect(page.getByText(/Synced with server/)).toBeVisible();
  await expect(page.locator(".diversion-list")).toContainText("submitted");

  const doc = await page.evaluate(() => fetch("/edits").then((r) => r.json()));
  const mine = doc.find(
    (r) => r.status === "submitted" && r.diversion.skipped_stops?.includes("C"),
  );
  expect(mine).toBeTruthy();
  expect(mine.diversion.added_secs).toBe(300);

  // Pull: a second client pushes a record of its own; our reload must
  // surface it through the bootstrap sync.
  await page.evaluate(async () => {
    const doc = await fetch("/edits").then((r) => r.json());
    const mine = doc.find(
      (r) => r.status === "submitted" && r.diversion.skipped_stops?.includes("C"),
    );
    const record = {
      ...mine,
      id: "e-injected",
      diversion: {
        ...mine.diversion,
        id: "d-injected",
        added_secs: 120,
        skipped_stops: [],
      },
    };
    const res = await fetch("/edits", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify([record]),
    });
    if (!res.ok) throw new Error(`inject failed: ${res.status}`);
  });

  await page.reload();
  await waitForMap(page);
  await expect(page.getByText(/Restored/)).toBeVisible({ timeout: 10_000 });
  // Bootstrap sync pulled the injected record (+2 min, no skips)
  // alongside our own (skips C, +5 min).
  await expect(page.locator(".diversion-list")).toContainText("skips Main & 3rd");
  await expect(page.locator(".diversion-list")).toContainText("+2 min");
});

test("persona tabs split the workflows", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  await loadFeed(page);

  // Rider: arrivals and trip planning, no ops tools.
  await expect(page.getByRole("heading", { name: "Trip planner" })).toBeVisible();
  await expect(page.getByRole("button", { name: "New diversion" })).toHaveCount(0);

  // Dispatcher: the operation — blocks and diversions, no trip planner.
  await switchPersona(page, "Dispatcher");
  await expect(page.getByRole("heading", { name: "Blocks" })).toBeVisible();
  await expect(page.getByRole("button", { name: "New diversion" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Trip planner" })).toHaveCount(0);

  // Planner: network authoring. Loading data is app config now, not
  // persona content — the Data sources panel is closed in the
  // workspace (its inputs stay attached for the config flows).
  await switchPersona(page, "Planner");
  await expect(page.getByRole("heading", { name: "Network document" })).toBeVisible();
  await expect(page.locator("#data-sources")).toBeHidden();
});

test("planner authors a network and the rider uses it", async ({ page }) => {
  await page.goto("/");
  await waitForMap(page);
  // The server's edit document is shared across this run (earlier
  // tests pushed diversions whose route/stop ids match the derived
  // network — they apply to it, by design). This test is about
  // authoring, not diversions: drop the local document.
  await page.evaluate(() => window.__clearEdits());
  await loadFeed(page);
  await switchPersona(page, "Planner");

  // The planner works off an existing network: derive the document
  // from the loaded feed (trips cluster into patterns).
  await page.getByRole("button", { name: "Derive from loaded feed" }).click();
  await expect(page.getByText("valid", { exact: true })).toBeVisible();

  // Drill into the service: the overview rail selects it, its editor
  // opens in the drawer, and the map frames the pattern.
  await page.getByRole("button", { name: "Open service WK:pat-10-0-0" }).click();
  await expect(page.locator(".planner-drawer")).toBeVisible();
  // The persona switch remounted the map — wait for the new instance.
  await waitForMap(page);
  await expect
    .poll(async () => (await mapData(page, "planner")).features.length)
    .toBeGreaterThan(0);

  // Move route 10's 08:00 trip (t10) five minutes later.
  const firstStart = page.locator(".trip-start").first();
  await firstStart.fill("08:05:00");
  await firstStart.blur();
  await expect(page.getByText("valid", { exact: true })).toBeVisible();

  // Materialize: the app's active feed is now the authored network.
  await page.getByRole("button", { name: "Load into app" }).click();
  await expect(page.getByText(/Materialized/)).toBeVisible();

  // The rider sees the authored schedule: the 08:05 departure at A.
  await page.evaluate(() => window.__setNow({ date: "2026-10-23", secs: 27000 }));
  await switchPersona(page, "Rider");
  await page.getByLabel("Stop").selectOption("A");
  await expect(page.getByText("08:05", { exact: true })).toBeVisible();

  // And rides it: A -> B boards the 08:05 trip, arrives 08:15 (the
  // authored 10-minute run).
  await page.getByLabel("From", { exact: true }).selectOption("A");
  await page.getByLabel("To", { exact: true }).selectOption("B");
  await page.locator("#plan-time").fill("07:30");
  await page.getByRole("button", { name: "Plan trip" }).click();
  await expect(page.locator(".plan-overlay").getByText("08:15", { exact: true })).toBeVisible();
});
