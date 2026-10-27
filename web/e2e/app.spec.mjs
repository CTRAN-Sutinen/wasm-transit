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
//   vehicle at 45.44/-73.44 on t10. sample-rt-moved.pb is the same
//   feed with the vehicle at 45.442 (encode_sample takes lat/lon) —
//   it proves the bus popup follows its vehicle across refreshes.

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

/// MapLibre projects in CANVAS coordinates; the sidebar layout
/// offsets the canvas inside the viewport. Viewport coordinates for
/// a lon/lat (with a pixel nudge), and its inverse.
async function mapViewportPoint(page, lonLat, dx = 0, dy = 0) {
  return page.evaluate(([lon, lat, dx, dy]) => {
    const r = window.__map.getCanvas().getBoundingClientRect();
    const p = window.__map.project([lon, lat]);
    return { x: Math.round(p.x + r.x) + dx, y: Math.round(p.y + r.y) + dy };
  }, [...lonLat, dx, dy]);
}
async function mapUnproject(page, x, y) {
  return page.evaluate(([x, y]) => {
    const r = window.__map.getCanvas().getBoundingClientRect();
    return window.__map.unproject([x - r.x, y - r.y]);
  }, [x, y]);
}

/// Pick an option in a Combobox (the searchable select replacement):
/// focus the box, type the query, click the first matching row.
/// `query` must prefix-match the option's label.
async function comboPick(page, label, query) {
  const box = page.getByRole("combobox", { name: label, exact: true });
  await box.click();
  await box.fill(query);
  await page.getByRole("option", { name: new RegExp(`^${query}`) }).first().click();
}

test("loads cleanly with no console errors", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(`pageerror: ${e}`));
  page.on("console", (m) => {
    if (m.type() === "error" && !m.text().includes("favicon")) {
      errors.push(m.text());
    }
  });
  await page.goto("/app.html");
  await expect(page.getByText(/capability bus ready/i)).toBeVisible();
  await waitForMap(page);
  expect(errors).toEqual([]);
});

test("loading a feed populates summary, board and map", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);

  // compact feed line (the persona split retired the summary table)
  await expect(page.locator(".feed-line")).toContainText("7 trips");

  // departures render for the first stop once selected
  await comboPick(page, "Stop", "Main & 1st");
  const rows = page.locator("section:has(h2:text('Departures')) tbody tr");
  await expect(rows).toHaveCount(3, { timeout: 10_000 });

  // stops are drawn on the map (the station pin and its two bays
  // included — the fixture models the parent/child shape)
  await expect
    .poll(async () => (await mapData(page, "stops")).features.length)
    .toBe(6);
});

test("clicking a departure draws the trip path", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);
  await comboPick(page, "Stop", "Main & 1st");
  const departures = page.locator("section", { has: page.getByRole("heading", { name: "Departures" }) });
  await departures.getByRole("row", { name: /08:00/ }).click();
  await expect
    .poll(async () => (await mapData(page, "trip")).features.length)
    .toBeGreaterThan(0);
});

test("trip planner draws the ridden segment, not the whole route", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);

  await comboPick(page, "From", "Main & 1st");
  await comboPick(page, "To", "Main & 2nd");
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
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);

  // B -> A has no service at any time (t10/t20 only run A -> B)
  await comboPick(page, "From", "Main & 2nd");
  await comboPick(page, "To", "Main & 1st");
  await page.getByRole("button", { name: "Plan trip" }).click();

  await expect(page.locator(".plan-notice")).toContainText(/No trip found/i);
  const itinerary = await mapData(page, "itinerary");
  expect(itinerary.features.length).toBe(0);
});

test("refresh restores the feed from local cache", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);

  await page.reload();
  await expect(page.getByText(/Restored/)).toBeVisible({ timeout: 10_000 });
  await expect(page.locator(".feed-line")).toContainText("7 trips");
});

test("realtime feed colors delays and vehicles", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);

  await page.locator(".dropzone input[type=file]").setInputFiles(path.join(fixtures, "sample-rt.pb"));

  // severity chip for the delayed BUS (worst +180 s -> "late 1-5 min"):
  // the chips count vehicles joined to trip delays, not raw trip
  // updates — one bus on t10, late 3 minutes.
  await expect(page.getByText(/late 1–5 min/)).toBeVisible();
  await expect(page.getByText("1 buses", { exact: true })).toBeVisible();

  // the vehicle body carries the joined delay, and the bus number
  // (VehicleDescriptor) rides on the map: label_text joins the route
  // badge to the vehicle label.
  const veh = (await mapData(page, "vehicles")).features[0];
  expect(veh.properties.delay).toBe(180);
  expect(veh.properties.label).toBe("4201");
  expect(veh.properties.label_text).toBe("10 · 4201");

  // Click the bus: its details popup — the bus number leads, then
  // trip, next stop, delay.
  await page.evaluate(() => window.__map.jumpTo({ center: [-73.44, 45.44], zoom: 15 }));
  const bus = await mapViewportPoint(page, [-73.44, 45.44]);
  await page.mouse.click(bus.x, bus.y);
  await expect(page.getByText("10 · 4201")).toBeVisible();
  await expect(page.getByText("Trip t10")).toBeVisible();
  await expect(page.getByText("Next stop Main & 1st")).toBeVisible();
  await expect(page.getByText("Delay +3 min late")).toBeVisible();

  // Dark mode must not blank the popup: the stock card hardcodes a
  // white background while the text color inherits — white-on-white
  // looked like an empty box. Any theme is fine as long as text and
  // card contrast.
  await page.emulateMedia({ colorScheme: "dark" });
  const popupPalette = await page
    .locator(".maplibregl-popup-content")
    .evaluate((el) => {
      const cs = getComputedStyle(el);
      return { color: cs.color, background: cs.backgroundColor };
    });
  expect(popupPalette.color).not.toBe(popupPalette.background);

  // The popup rides with its bus: refresh the feed with the vehicle
  // moved, and the open popup re-anchors to the new position (and
  // keeps its content).
  await page.locator(".dropzone input[type=file]").setInputFiles(path.join(fixtures, "sample-rt-moved.pb"));
  await expect(async () => {
    const veh = (await mapData(page, "vehicles")).features[0];
    // f32 coordinates: closeTo, not exact equality.
    expect(veh.geometry.coordinates[0]).toBeCloseTo(-73.44, 3);
    expect(veh.geometry.coordinates[1]).toBeCloseTo(45.442, 3);
  }).toPass();
  const anchor = await mapViewportPoint(page, [-73.44, 45.442]);
  const rect = await page.evaluate(() => {
    const r = document.querySelector(".maplibregl-popup").getBoundingClientRect();
    return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
  });
  // anchored at the new position…
  expect(anchor.x).toBeGreaterThanOrEqual(rect.left - 10);
  expect(anchor.x).toBeLessThanOrEqual(rect.right + 10);
  expect(anchor.y).toBeGreaterThanOrEqual(rect.top - 20);
  expect(anchor.y).toBeLessThanOrEqual(rect.bottom + 20);
  // …not the old one, and still telling the story.
  const oldAnchor = await mapViewportPoint(page, [-73.44, 45.44]);
  expect(oldAnchor.y).toBeGreaterThan(rect.bottom);
  await expect(page.locator(".bus-popup").getByText("10 · 4201")).toBeVisible();

  // the departure board predicts the delayed time: 08:00 + 120 s
  await comboPick(page, "Stop", "Main & 1st");
  await expect(page.getByText("08:02")).toBeVisible();
  await expect(page.getByText("+2 min")).toBeVisible();
});

/// Create a diversion through the editor UI: route 10, anchors
/// A -> B, interior stop C auto-skipped, `mins` minutes added.
async function makeDiversion(page, mins) {
  await page.getByRole("button", { name: "New diversion" }).click();
  await comboPick(page, "Route", "10");
  // Click the two anchors in the stop-sequence list (A, C, B).
  await page.locator(".stop-seq").getByRole("button", { name: "Main & 1st" }).click();
  await page.locator(".stop-seq").getByRole("button", { name: "Main & 2nd" }).click();
  await page.getByLabel("Added time (min)").fill(String(mins));
  await page.getByRole("button", { name: "Apply diversion" }).click();
  await expect(page.getByText(/Diversion applied: 3 trips/)).toBeVisible();
}

test("diversion reroutes the schedule, the board and the planner", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);

  // Route 10, anchors A -> B, skip C, +5 minutes — dispatcher work.
  await switchPersona(page, "Dispatcher");
  await makeDiversion(page, 5);

  // The rider's view: C loses its only service (t30 used to run A -> C -> B).
  await switchPersona(page, "Rider");
  await comboPick(page, "Stop", "Main & 3rd");
  const rows = page.locator("section:has(h2:text('Departures')) tbody tr");
  await expect(rows).toHaveCount(0);

  // B is where every fixture trip ENDS: an arrival is not a
  // departure, so B's board stays empty — the diverted arrival
  // (08:15) shows in the planner below, never as a bus leaving B.
  await comboPick(page, "Stop", "Main & 2nd");
  await expect(rows).toHaveCount(0);

  // The planner rides the diverted schedule and shows the delta.
  await comboPick(page, "From", "Main & 1st");
  await comboPick(page, "To", "Main & 2nd");
  await page.locator("#plan-time").fill("07:30");
  await page.getByRole("button", { name: "Plan trip" }).click();
  await expect(page.locator(".plan-overlay")).toBeVisible();
  await expect(page.locator(".plan-overlay").getByText("08:15", { exact: true })).toBeVisible();
  await expect(page.locator(".plan-compare")).toContainText(/without the active diversion: arrive 08:10\s+\(5 min later\)/i);

  // A -> C is unreachable while the diversion is active; the notice
  // says what the baseline looked like.
  await comboPick(page, "To", "Main & 3rd");
  await page.getByRole("button", { name: "Plan trip" }).click();
  await expect(page.locator(".plan-notice")).toContainText(/without it you would arrive 08:45/);

  // Toggle the diversion off (dispatcher again): C's service returns.
  await switchPersona(page, "Dispatcher");
  await page.locator(".diversion-list input[type=checkbox]").first().click();
  // Retired leaves the working list (a tombstone behind the
  // disclosure) — reveal it to assert it survives the reload.
  await page.getByRole("button", { name: /Show \d+ retired/ }).click();
  await expect(page.locator(".diversion-list li.inactive")).toHaveCount(1);
  await switchPersona(page, "Rider");
  await comboPick(page, "Stop", "Main & 3rd");
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
  await page.getByRole("button", { name: /Show \d+ retired/ }).click();
  await expect(page.locator(".diversion-list li.inactive")).toHaveCount(1);
});

test("diversion ripple shows as a tight block layover", async ({ page }) => {
  await page.goto("/app.html");
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

test("comboboxes search, and blocks scope to the service day", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page); // Friday 2026-10-23, 07:30

  // The stop picker is a combobox: typing narrows the options (the
  // debounce lets the list settle), picking re-targets the board.
  const stopBox = page.getByRole("combobox", { name: "Stop", exact: true });
  await stopBox.click();
  await expect(page.getByRole("option")).toHaveCount(6);
  await stopBox.fill("3rd");
  await expect(page.getByRole("option")).toHaveCount(1);
  await page.getByRole("option", { name: /^Main & 3rd/ }).click();
  const rows = page.locator("section:has(h2:text('Departures')) tbody tr");
  await expect(rows).toHaveCount(1, { timeout: 10_000 }); // t30 at C: 08:45

  // A STATION PIN answers the station board: the parent has no
  // trips of its own, so the per-stop path is empty and the board
  // falls back to cap-board's pooled station_departures —
  // both bays' departures, each row tagged with its bay.
  await stopBox.click();
  await stopBox.fill("Main Station");
  await page.getByRole("option", { name: /Main Station\s*STN/ }).click();
  await expect(rows).toHaveCount(2, { timeout: 10_000 }); // t40 leaves SA, t41 leaves SB
  await expect(page.getByText(/Station board/)).toBeVisible();
  await expect(page.locator("section:has(h2:text('Departures'))")).toContainText("SA");
  await expect(page.locator("section:has(h2:text('Departures'))")).toContainText("SB");

  // The runcut view is model-native: B2's only trip (t12) is
  // single-stop — the model cannot represent it, the import drops it
  // (and reports it in its notes) — so the day's blocks list does
  // not offer B2 at all. The honest hole, never a silent blank:
  // the picker offers exactly the vehicles with authored trips
  // running, with that day's counts.
  await switchPersona(page, "Dispatcher");
  const blockBox = page.getByRole("combobox", { name: "Block", exact: true });
  await blockBox.click();
  await blockBox.fill("");
  await expect(page.getByRole("option", { name: /^B1/ })).toBeVisible();
  await expect(page.getByRole("option")).toHaveCount(1);

  // Saturday: WK is off and the holiday was Friday-only — every bus
  // is parked, so there is nothing to pick and no table to show.
  await page.evaluate(() => window.__setNow({ date: "2026-10-24", secs: 27000 }));
  await expect(page.getByRole("heading", { name: "Blocks" })).toBeHidden();
});

test("status text stays on one line in every persona", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);
  await page.locator(".dropzone input[type=file]").setInputFiles(path.join(fixtures, "sample-rt.pb"));
  await expect(page.getByText("1 buses", { exact: true })).toBeVisible();

  // Chips, badges and table cells hold single horizontal tokens —
  // any of them spilling onto a second line is unreadable (the
  // realtime bus chip shipped wrapped once). The fixture texts are
  // short, so the guard is structural: no wrapping allowed, and the
  // rendered text must actually sit on one line, in every persona.
  for (const persona of ["Rider", "Dispatcher", "Planner"]) {
    await switchPersona(page, persona);
    const spills = await page.evaluate(() => {
      const bad = [];
      for (const el of document.querySelectorAll(".chip, .route-badge, .feed-line, td, th")) {
        if (!el.textContent?.trim()) continue;
        const cs = getComputedStyle(el);
        if (cs.whiteSpace !== "nowrap") {
          bad.push(`white-space:${cs.whiteSpace} on ${el.className || el.tagName}`);
          continue;
        }
        const range = document.createRange();
        range.selectNodeContents(el);
        const rects = [...range.getClientRects()];
        if (rects.length) {
          // A second line sits a full line-height below the first;
          // the sub-pixel offset between an inline-block box and its
          // inner line box must not count as a wrap.
          const top = Math.min(...rects.map((r) => r.top));
          const bottom = Math.max(...rects.map((r) => r.bottom));
          const height = Math.max(...rects.map((r) => r.height));
          if (bottom - top > height * 1.5) bad.push(`wrapped: ${el.textContent.trim().slice(0, 40)}`);
        }
      }
      return bad;
    });
    expect(spills, `${persona} spilled text`).toEqual([]);
  }
});

test("street network import paginates ArcGIS-style queries", async ({ page }) => {
  await page.goto("/app.html");
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

test("population layer import registers through the analysis capability", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);

  // No feed needed: reference data, loadable on its own. The layer
  // registers in cap-analysis under its content hash — the
  // registration answer IS the import handle (points, the picked
  // pop_field, the total), the monolith's parse-only op is gone.
  await page.locator("#population-file").setInputFiles(path.join(fixtures, "population.geojson"));

  await expect(page.getByText("Coverage analysis is on.")).toBeVisible();
  const chip = page.locator(".chip.svc", { hasText: "points" });
  await expect(chip).toContainText("6 points");
  await expect(chip).toContainText("2,100 people (POP)");
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
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);
  await switchPersona(page, "Dispatcher");
  await settleOnStops(page);

  await page.getByRole("button", { name: "New diversion" }).click();

  // Selecting a route shows its alignment — the ghost the detour is
  // drawn against (t30 is route 10's representative: A → C → B).
  await comboPick(page, "Route", "10");
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
  const near = await mapViewportPoint(page, [-73.5060, 45.5100], 10, 6);
  const rawNear = await mapUnproject(page, near.x, near.y);
  await page.mouse.click(near.x, near.y);

  // With a street network loaded, a click far from every street and
  // stop places NOTHING — off-network waypoints are not drivable.
  const far = await mapViewportPoint(page, [-73.5060, 45.5100], -200, 100);
  await page.mouse.click(far.x, far.y);
  await expect(page.getByText(/off-network points are ignored/)).toBeVisible();

  const drawn = (await mapData(page, "detour-draft")).features.filter(
    (f) => f.geometry.type === "Point",
  );
  expect(drawn.length).toBe(1); // only the snapped street click

  const [snappedLon, snappedLat] = drawn[0].geometry.coordinates;
  const snappedDist = Math.min(...sideSt.map(([a, b]) => distToSeg(snappedLon, snappedLat, a, b)));
  expect(snappedDist).toBeLessThan(2e-6); // on the street geometry
  expect(Math.hypot(snappedLon - rawNear.lng, snappedLat - rawNear.lat))
    .toBeGreaterThan(1e-5); // not the raw click

  // Click-to-confirm: two clicks along Side St; the second click
  // ROUTES through the graph, so the trace commits Side St's own
  // vertices — including v1 (-73.5060, 45.5100), crossed on the way.
  const start = await mapViewportPoint(page, [-73.5025, 45.5065]); // mid first segment
  const end = await mapViewportPoint(page, [-73.5095, 45.5135]); // mid second segment
  await page.mouse.click(start.x, start.y);
  // Between clicks nothing commits — the routed path to the cursor is
  // only PREVIEWED (a line + the snapped cursor dot).
  await page.mouse.move(end.x - 8, end.y - 8);
  const preview = (await mapData(page, "detour-preview")).features;
  expect(preview.length).toBe(2);
  await page.mouse.click(end.x, end.y);
  const stroked = (await mapData(page, "detour-draft")).features.filter(
    (f) => f.geometry.type === "Point",
  );
  expect(stroked.length).toBeGreaterThanOrEqual(4); // 3 clicks + routed corner
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
  await page.goto("/app.html");
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
  const clickAt = (lonLat) => mapViewportPoint(page, lonLat);
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
  await page.goto("/app.html");
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
  await page.goto("/app.html");
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
  await expect(page.getByRole("heading", { name: "Service plans" })).toBeVisible();
  await expect(page.locator("#data-sources")).toBeHidden();

  // The map is ONE canvas across personas: switching never remounts
  // it (the camera would visibly jump and every source would clear).
  const mapId = await page.evaluate(() => {
    window.__map.__marker = Symbol ? String(Math.random()) : "x";
    return window.__map.__marker;
  });
  await switchPersona(page, "Rider");
  const sameMap = await page.evaluate((id) => window.__map.__marker === id, mapId);
  expect(sameMap).toBe(true);
});

// Stage C: the analysis ops are DELETED from the monolith — the
// planner's numbers exist only through the capability. When the
// module cannot assemble, the planner degrades loudly (authoring
// continues, numbers are off) — the bus's per-capability degradation,
// now load-bearing for a real screen.
test("planner degrades when the analysis capability cannot assemble", async ({ page }) => {
  await page.route(/caps\/analysis-.*\.wasm/, (route) => route.abort());
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);
  await switchPersona(page, "Planner");
  await page.getByRole("button", { name: "Derive from loaded feed" }).click();
  await expect(page.getByText("valid", { exact: true })).toBeVisible();
  // Numbers are off — loudly, not silently; authoring continues.
  await expect(page.getByText(/Analysis capability unavailable/)).toBeVisible();
  expect(await page.evaluate(() => window.__analysisPath())).toBeNull();
});

// The runcut reads moved to the bus: BlocksPanel (the blocks list and
// one block's chain) answers through cap-runcut over the network
// document — it no longer needs the gtfs capability at all. When the
// runcut module cannot assemble the view is OFF and says so; the
// rider board beside it reads cap-board (a different module), so it
// keeps answering: the assembly degrades per capability, never
// wholesale.
test("the blocks view degrades when the runcut capability cannot assemble", async ({ page }) => {
  await page.route(/caps\/runcut-.*\.wasm/, (route) => route.abort());
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);
  await switchPersona(page, "Dispatcher");
  await expect(
    page
      .getByText(/blocks view needs the runcut capability|runcut capability unavailable/)
      .first(),
  ).toBeVisible({ timeout: 10_000 });
  // The rider board beside it still answers — through cap-board,
  // which composes over cap-network, not cap-runcut.
  await switchPersona(page, "Rider");
  await comboPick(page, "Stop", "Main & 1st");
  await expect(
    page.locator("section:has(h2:text('Departures')) tbody tr"),
  ).toHaveCount(3, { timeout: 10_000 });
});

// The board's own dependency: the day view answers through cap-board
// over the network document. The network module itself stays
// assembled (the directory reads it for the pickers — a different
// capability, unaffected), so the stop picker works while the board
// is OFF and says so — the monolith op is deleted, there is no
// fallback to silently serve instead.
test("the board degrades when the board capability cannot assemble", async ({ page }) => {
  await page.route(/caps\/board-.*\.wasm/, (route) => route.abort());
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);
  await switchPersona(page, "Rider");
  await comboPick(page, "Stop", "Main & 1st");
  await expect(page.getByText(/board needs the network and board/)).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.locator("section:has(h2:text('Departures')) tbody tr")).toHaveCount(0);
});

// Stage B of the SECOND extraction (the codec): bootstrap, validation
// and the publish path live in the NETWORK CAPABILITY — the monolith
// has no copy. When the module cannot assemble, every one of them
// degrades loudly: bootstrap refuses with an honest status, and a
// session that loses the capability mid-flight flips the chip to
// "validation off" instead of claiming validity it cannot check,
// while materialize refuses rather than loading unchecked.
test("planner degrades when the network capability cannot assemble", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);
  await switchPersona(page, "Planner");

  // Bootstrap assembles: the derived document is valid (the chip
  // says so — the capability is up).
  await page.getByRole("button", { name: "Derive from loaded feed" }).click();
  await expect(page.getByText("valid", { exact: true })).toBeVisible();

  // Now the capability goes away mid-session (unfetchable + forgotten):
  // the next composition must degrade, never silently serve stale
  // claims.
  await page.route(/caps\/network-.*\.wasm/, (route) => route.abort());
  await page.evaluate(() => window.__forgetCapability("network"));

  // Materialize refuses — export is the capability's op.
  await page.getByRole("button", { name: "Document" }).click();
  await page.getByRole("button", { name: "Load into app" }).click();
  await expect(page.getByText(/Network capability unavailable — cannot materialize/)).toBeVisible();

  // And validation is OFF: the materialize attempt re-validated
  // through the missing capability and turned the chip off (the chip
  // says so — it must not claim validity nobody can check).
  await expect(page.getByText("validation off", { exact: true })).toBeVisible();
});

test("planner authors a network and the rider uses it", async ({ page }) => {
  await page.goto("/app.html");
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

  // Stage B: the planner's numbers come from the ANALYSIS CAPABILITY
  // when the bus assembles it — same domain code, extracted from the
  // monolith, registered once per edit and queried by hash.
  await expect
    .poll(async () => page.evaluate(() => window.__analysisPath()))
    .toBe("bus");

  // The day summary — network_stats finally has a screen (dead UI
  // surface its whole life): the picked day's shape on one line.
  // Friday 2026-10-23: five trips, three routes, five stops served.
  await expect(page.getByText("5 trips", { exact: true })).toBeVisible();
  await expect(page.getByText("3 routes", { exact: true })).toBeVisible();
  await expect(page.getByText("5 stops", { exact: true })).toBeVisible();
  await expect(page.getByText(/0\.7 revenue hours/)).toBeVisible();
  await expect(page.getByText(/span 08:00:00–08:50:00/)).toBeVisible();

  // The service type screen shows the type's WHOLE day — routes,
  // schedules, stops — and frames the type's network on the map
  // (every pattern scheduled under it).
  await page.getByRole("button", { name: "Open service type WK" }).click();
  await expect(page.getByRole("heading", { name: /Service type/ })).toBeVisible();
  await expect(page.getByText("Routes running")).toBeVisible();
  await expect(page.getByText("Schedules (5)")).toBeVisible();
  await expect(page.locator(".map-side")).toBeVisible();
  // cap-directory's focus answers it: one line per pattern scheduled
  // under WK (five), its stops as points.
  await expect
    .poll(async () => (await mapData(page, "planner")).features.filter((f) => f.properties.kind === "line").length)
    .toBe(5);

  // Pure data screens (Document) hide the map and the rail takes the
  // whole width — no dead column next to it (regression: an invalid
  // grid rule left the shell two-tracked, rendering a black void
  // beside the rail).
  await page.getByRole("button", { name: "Document" }).click();
  await expect(page.locator(".map-side")).toBeHidden();
  const railWidth = await page.locator(".sidebar").evaluate((el) => el.getBoundingClientRect().width);
  expect(railWidth, "the rail absorbs the hidden map's width").toBeGreaterThan(700);
  // Its counts are cap-directory's rows too (W6), never document reads.
  await expect(page.locator(".chip", { hasText: /^[1-9]\d* stops$/ })).toBeVisible();

  // The list screens, the route screen, and the stop screen are
  // cap-directory answers (W5, W6): the filter narrows by every term; the route lists its
  // patterns, the stops they visit, and its schedules by service
  // type with each one's trip count.
  await page.getByRole("button", { name: "Stops", exact: true }).click();
  await page.getByLabel("Filter stops").fill("main 2nd");
  await expect(page.getByRole("button", { name: /^Open stop / })).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Open stop B" })).toBeVisible();
  // The stop's serving routes are cap-directory's stop_detail (W6).
  await page.getByRole("button", { name: "Open stop B" }).click();
  await expect(page.getByRole("button", { name: "Open route 10" })).toBeVisible();
  await page.getByRole("button", { name: "Routes", exact: true }).click();
  await page.getByRole("button", { name: "Open route 10" }).click();
  await expect(page.getByText("Patterns (2)")).toBeVisible();
  await expect(page.getByText("Stops (3)")).toBeVisible();
  await expect(page.getByRole("button", { name: "Open schedule WK:pat-10-0-1" })).toHaveText("1 trips");

  // The workspace is a screen stack: menu → Schedules, drill into the
  // schedule, the editor is a screen in the rail (Back pops it).
  await page.getByRole("button", { name: "Schedules" }).click();
  await page.getByRole("button", { name: "Open schedule WK:pat-10-0-0" }).click();
  await expect(page.getByRole("heading", { name: /Schedule/ })).toBeVisible();

  // Move route 10's 08:00 trip (t10) five minutes EARLIER. t10 is in
  // block B1 with a 10-minute layover before t20 (08:20): later would
  // eat the layover (validate reports layover_infeasible — the old
  // validator missed it because the block kept a stale reference,
  // QAF-1); earlier keeps the runcut feasible, and the block's
  // reference moves with the trip.
  const firstStart = page.locator(".trip-start").first();
  await firstStart.fill("07:55:00");
  await firstStart.blur();

  // Materialize from the Document screen: the app's active feed is
  // now the authored network.
  await page.getByRole("button", { name: "Document" }).click();
  await expect(page.getByText("valid", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Load into app" }).click();
  await expect(page.getByText(/Materialized/)).toBeVisible();

  // The rider sees the authored schedule: the 07:55 departure at A.
  await page.evaluate(() => window.__setNow({ date: "2026-10-23", secs: 27000 }));
  await switchPersona(page, "Rider");
  await comboPick(page, "Stop", "Main & 1st");
  await expect(page.getByText("07:55", { exact: true })).toBeVisible();

  // And rides it: A -> B boards the 07:55 trip, arrives 08:05 (the
  // authored 10-minute run).
  await comboPick(page, "From", "Main & 1st");
  await comboPick(page, "To", "Main & 2nd");
  await page.locator("#plan-time").fill("07:30");
  await page.getByRole("button", { name: "Plan trip" }).click();
  await expect(page.locator(".plan-overlay").getByText("08:05", { exact: true })).toBeVisible();

  // Creation is a gesture the AUTHORING CAPABILITY applies: the id is
  // minted in the module (the first free s<n>), reported back, and
  // the new stop's editor opens on it.
  await switchPersona(page, "Planner");
  await page.getByRole("button", { name: "Stops" }).click();
  await page.getByRole("button", { name: "Add stop" }).click();
  await expect(page.getByRole("heading", { name: "Stop s1" })).toBeVisible();
});

test("a malformed saved record cannot wedge apply and retire", async ({ page }) => {
  // A document written by an older build — junk entries and records
  // missing serde-defaulted fields. Restore must DROP the junk, FILL
  // the defaults, and leave Apply/Retire working (a malformed entry
  // used to throw inside cloneEdits: both buttons died silently).
  test.setTimeout(90_000);
  await page.goto("/app.html");
  await page.evaluate(async () => {
    const junk = [
      { id: "junk-entry", status: "draft", rev: 1, updated_at: 0 }, // no diversion
      {
        id: "old-shape", status: "retired", rev: 1, updated_at: 1, // pre-detour build
        diversion: {
          id: "div-old", route_id: "10", first_kept_stop: "A", last_kept_stop: "B",
          start_date: "2026-10-23", end_date: "2026-10-23",
        },
      },
      {
        id: "junk-detour", status: "draft", rev: 1, updated_at: 0, // detour is not a polyline
        diversion: {
          id: "div-j", route_id: "10", first_kept_stop: "A", last_kept_stop: "B",
          start_date: "2026-10-23", end_date: "2026-10-23", detour: "polyline??",
        },
      },
    ];
    await new Promise((resolve, reject) => {
      const req = indexedDB.open("keyval-store", 1); // idb-keyval's default store
      req.onupgradeneeded = () => req.result.createObjectStore("keyval");
      req.onsuccess = () => {
        const tx = req.result.transaction("keyval", "readwrite");
        tx.objectStore("keyval").put(junk, "transit:edits");
        tx.oncomplete = resolve;
        tx.onerror = () => reject(tx.error);
      };
      req.onerror = () => reject(req.error);
    });
  });
  await page.reload();
  await waitForMap(page);
  await loadFeed(page);
  await switchPersona(page, "Dispatcher");

  // The junk is gone and the old-shape record survived (filled): no
  // "undefined" chips. It is retired, so it lives behind the
  // disclosure — reveal it.
  await page.getByRole("button", { name: /Show \d+ retired/ }).click();
  const listText = await page.locator(".diversion-list").innerText();
  expect(listText).toContain("Main & 1st → Main & 2nd");
  expect(listText).not.toContain("undefined");
  expect(listText).not.toContain("junk");
  await page.getByRole("button", { name: /Hide \d+ retired/ }).click();

  // Apply works (this is the button that was dead).
  await makeDiversion(page, 7);
  // Retire works: the record LEAVES the working list...
  const item = page.locator(".diversion-list li", { hasText: "+7 min" });
  await item.locator("input[type=checkbox]").click();
  await expect(item).toHaveCount(0);
  // ...and the tombstone is revealable and restorable.
  await page.getByRole("button", { name: /Show \d+ retired/ }).click();
  const tombstone = page.locator(".diversion-list li.inactive", { hasText: "+7 min" });
  await expect(tombstone).toBeVisible();
  await tombstone.locator("input[type=checkbox]").click();
  await expect(page.locator(".diversion-list li", { hasText: "+7 min" })).toBeVisible();
});

test("escape exits tracing mode", async ({ page }) => {
  test.setTimeout(90_000);
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);
  await switchPersona(page, "Dispatcher");
  await settleOnStops(page);

  await page.getByRole("button", { name: "New diversion" }).click();
  await comboPick(page, "Route", "10");
  await page.getByRole("button", { name: "Trace path on map" }).click();
  await expect(page.getByText("click confirms · drag pans")).toBeVisible();

  // ESC: the drawing reflex. The draft survives; only the mode ends.
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "Trace path on map" })).toBeVisible();

  // Off-mode clicks commit nothing.
  const center = await page.evaluate(() => window.__map.getCenter());
  const c = await mapViewportPoint(page, [center.lng, center.lat]);
  await page.mouse.click(c.x, c.y);
  const pts = await page.evaluate(() => {
    const d = window.__map.getSource("detour-draft").serialize().data;
    return d.features.filter((f) => f.geometry.type === "Point").length;
  });
  expect(pts).toBe(0);
});

test("a selected trip can always be cleared from the map", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);
  await settleOnStops(page);

  // Select t30 at stop A (08:40 — the first anchor, so leftover
  // diversions from other tests do not shift it): its route line
  // appears. (The first row can be a one-stop trip — no geometry.)
  await comboPick(page, "Stop", "Main & 1st");
  await page.locator(".clickable-row", { hasText: "08:40" }).click();
  await expect
    .poll(async () => (await mapData(page, "trip")).features.length)
    .toBeGreaterThan(0);

  // Clicking the row again deselects — the line leaves the map.
  await page.locator(".clickable-row", { hasText: "08:40" }).click();
  await expect
    .poll(async () => (await mapData(page, "trip")).features.length)
    .toBe(0);

  // And a fresh trip plan supersedes a lingering selection: the
  // itinerary replaces the trip line, not adds to it.
  await page.locator(".clickable-row", { hasText: "08:40" }).click();
  await expect
    .poll(async () => (await mapData(page, "trip")).features.length)
    .toBeGreaterThan(0);
  await comboPick(page, "From", "Main & 1st");
  await comboPick(page, "To", "Main & 2nd");
  await page.locator("#plan-time").fill("07:30");
  await page.getByRole("button", { name: "Plan trip" }).click();
  await expect(page.locator(".plan-overlay")).toBeVisible();
  expect((await mapData(page, "trip")).features.length).toBe(0);
  expect((await mapData(page, "itinerary")).features.length).toBeGreaterThan(0);
});

test("hiding the panel gives the map the full screen", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);

  // Collapse: the sidebar leaves the grid and the map takes every
  // pixel — the canvas resizes with its container (a grid-item bug
  // once left the map in a dead 0px column behind a black screen).
  await page.getByRole("button", { name: "Hide panel" }).click();
  await expect(page.getByRole("button", { name: "Show panel" })).toBeVisible();
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const r = document.querySelector(".map-side").getBoundingClientRect();
        return { w: Math.round(r.width), h: Math.round(r.height) };
      }),
    )
    .toEqual({ w: page.viewportSize().width, h: page.viewportSize().height });
  await expect
    .poll(async () => page.evaluate(() => window.__map.getCanvas().getBoundingClientRect().width))
    .toBeGreaterThan(1000);

  // Restore: the sidebar returns, the map resizes back.
  await page.getByRole("button", { name: "Show panel" }).click();
  await expect(page.getByRole("heading", { name: "Departures" })).toBeVisible();
  await expect
    .poll(async () =>
      page.evaluate(() => window.innerWidth - document.querySelector(".map-side").getBoundingClientRect().width),
    )
    .toBeGreaterThan(300);
});

test("service plans frame service changes and survive reload", async ({ page }) => {
  await page.goto("/app.html");
  await waitForMap(page);
  await loadFeed(page);
  await switchPersona(page, "Planner");

  // The workspace roots at the plan chain.
  await expect(page.getByRole("heading", { name: "Service plans" })).toBeVisible();
  await page.getByRole("button", { name: "Derive from loaded feed" }).click();
  await expect(page.getByText("valid", { exact: true })).toBeVisible();

  // Frame the derived network as a pick: name + effective date.
  await page.getByRole("button", { name: "Document" }).click();
  await page.getByLabel("Plan name").fill("Spring 2026");
  await page.getByLabel("Plan effective date").fill("2026-03-01");
  // Spring is in force in June; the fall pick is still a target.
  await page.evaluate(() => window.__setNow({ date: "2026-06-01", secs: 0 }));

  // Fork the next pick — the service-change workflow.
  await page.getByRole("button", { name: "Plans" }).click();
  await page.getByLabel("New pick name").fill("Fall 2026 pick");
  await page.getByLabel("New pick effective date").fill("2026-09-07");
  await page.getByRole("button", { name: "Fork into the next pick" }).click();
  await expect(page.getByText(/Forked into/)).toBeVisible();

  // The chain resolves and derives the governing periods.
  await page.getByRole("button", { name: "Plans" }).click();
  await expect(page.getByText("in force", { exact: true })).toBeVisible();
  await expect(page.getByText("2026-03-01 → 2026-09-07")).toBeVisible();
  await expect(page.getByText("2026-09-07 → open")).toBeVisible();

  // Plans are documents: they outlive the session.
  await page.reload();
  await waitForMap(page);
  await switchPersona(page, "Planner");
  await page.getByRole("button", { name: "Plans" }).click();
  await expect(page.getByRole("button", { name: "Open plan Fall 2026 pick" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Open plan Spring 2026" })).toBeVisible();
  await expect(page.getByText("2026-03-01 → 2026-09-07")).toBeVisible();
});
