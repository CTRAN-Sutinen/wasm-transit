// The ride-now UI (src/uis/ride-now; docs/architecture/generated-uis/ride-now.md).
// The time is Friday 2026-10-23 at 07:55 in America/Montreal. The trip updates
// (sample-rt.pb) put t10 at +120 s at A and +180 s at B, for every live feed
// URL. The test serves the live feeds and the edit document, because the shared
// e2e server holds other specs' edits. cap-router's
// `a_cancelled_trip_is_never_ridden` pins that plans never ride a cancel.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

test.use({
  timezoneId: "UTC",
  geolocation: { latitude: 45.515, longitude: -73.515 },
  permissions: ["geolocation"],
});

const cancelT10 = [
  {
    id: "c-ride-now",
    rev: 1,
    status: "accepted",
    updated_at: 1,
    cancel: { id: "cancel-t10", trip_id: "t10", start_secs: null, date: "2026-10-23" },
  },
];

const detour = [
  {
    id: "e-ride-now",
    rev: 1,
    status: "accepted",
    updated_at: 1,
    diversion: {
      id: "d-ride-now",
      route_id: "10",
      direction_id: null,
      start_date: "2026-10-23",
      end_date: "2026-10-23",
      first_kept_stop: "A",
      last_kept_stop: "B",
      skipped_stops: ["C"],
      added_secs: 300,
      detour: null,
    },
  },
];

async function open(page, { edits = [], staleAsOf = null, editsVersion = 2 } = {}) {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  const headers = staleAsOf ? { "X-Store-Stale": "true", "X-Store-As-Of": String(staleAsOf) } : {};
  await page.route("**/edits", (route) =>
    route.fulfill({ contentType: "application/json", headers, body: JSON.stringify({ kind: "edit_records", schema_version: editsVersion, records: edits }) }),
  );
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
  await page.goto("/ride-now.html");
  await page.getByLabel("Agency GTFS zip").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  await expect(page.getByLabel("stop name")).toBeVisible({ timeout: 10_000 });
}

async function pick(page, field, text, name) {
  const input = page.getByLabel(field, { exact: true });
  await input.fill(text);
  await expect(page.getByRole("option").first()).toContainText(name);
  await input.press("Enter");
}

test("ride now answers near me, a stop, a save, and a trip", async ({ page }) => {
  await open(page);

  await expect(page.getByTestId("live-feeds")).toContainText("Live");

  const nearby = page.getByTestId("nearby");
  const first = nearby.getByTestId("stop-card").first();
  await expect(first).toContainText("Main Station");
  await expect(first).toContainText("8:15");

  // The type-ahead answers a typo.
  await page.getByLabel("stop name").fill("mian 2nd");
  await expect(page.getByRole("option").first()).toContainText("Main & 2nd");
  await page.getByLabel("stop name").fill("zebra crossing");
  await expect(page.getByRole("listbox")).toContainText("No stop is named “zebra crossing”");
  await pick(page, "stop name", "main 1s", "Main & 1st");

  const board = page.getByTestId("rider-board");
  await expect(board.locator("h2")).toHaveText("Main & 1st");
  const row = board.getByTestId("board-rows").locator("li").first();
  await expect(row).toContainText("8:02");
  await expect(row).toContainText("2 min late");
  await expect(row.locator("[data-status]")).toHaveAttribute("data-status", "late");
  await expect(board).toContainText("Live predictions");

  await board.getByRole("button", { name: "Save this stop" }).click();
  await expect(board.getByRole("button", { name: "Remove from saved" })).toBeVisible();
  await board.getByRole("button", { name: "back" }).click();
  const saved = page.getByTestId("saved");
  await expect(saved).toContainText("Main & 1st");
  await expect(saved).toContainText("8:02");
  await expect(saved).toContainText("2 min late");
  const recent = page.getByTestId("recent").getByRole("button", { name: "Main & 1st" });
  await expect(recent).toBeVisible();

  await recent.click();
  await board.getByRole("button", { name: "Directions from here" }).click();
  const trip = page.getByTestId("trip-plan");
  await expect(page.getByLabel("From", { exact: true })).toHaveValue("Main & 1st");
  await pick(page, "To", "main 2", "Main & 2nd");
  const it = trip.getByTestId("itinerary");
  await expect(it).toContainText("8:02");
  await expect(it).toContainText("2 min late");
  await expect(it).toContainText("Arrive Main & 2nd at 8:13 am");
  // Door to door.
  await expect(trip.getByTestId("duration")).toHaveText("11 min");
  await expect(trip.getByTestId("leave")).toHaveText("Leave in 7 min");
  await expect(page.getByTestId("ride-map")).toHaveAttribute("data-legs", "1");

  await trip.getByRole("radio", { name: "Last bus" }).click();
  await expect(trip.getByTestId("last-bus")).toBeVisible();
  await expect(it).toContainText("8:40");

  await trip.getByRole("radio", { name: "Arrive by" }).click();
  await trip.locator("input[type=time]").fill("08:35");
  await expect(it).toContainText("Arrive Main & 2nd at 8:30 am");

  // One linked module answers every capability, and holds the network once.
  const bus = (fn, ...args) => page.evaluate(([fn, args]) => window.__bus[fn](...args), [fn, args]);
  const caps = await bus("caps");
  expect(caps).toEqual(expect.arrayContaining(["board", "directory", "rt", "router"]));
  for (const cap of caps) expect(await bus("module", cap)).toBe("ui:ride-now");
  const networks = (await bus("held", "router")).held.filter((e) => e.kind === "network");
  expect(networks).toHaveLength(1);
});

test("ride now keeps saved stops across a reload", async ({ page }) => {
  await open(page);
  await pick(page, "stop name", "main 1s", "Main & 1st");
  const board = page.getByTestId("rider-board");
  await board.getByRole("button", { name: "Save this stop" }).click();
  await expect(board.getByRole("button", { name: "Remove from saved" })).toBeVisible();

  await page.reload();
  await page.getByLabel("Agency GTFS zip").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  const saved = page.getByTestId("saved");
  await expect(saved).toContainText("Main & 1st");
  await expect(saved).toContainText("8:02");
});

/// Writes idb-keyval's default store and localStorage before the page's own scripts run.
async function seedDevice(page, { documents, local }) {
  await page.goto("/manifest");
  await page.evaluate(
    ([documents, local]) => {
      for (const [k, v] of Object.entries(local)) localStorage.setItem(k, v);
      return new Promise((resolve, reject) => {
        const open = indexedDB.open("keyval-store");
        open.onupgradeneeded = () => open.result.createObjectStore("keyval");
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const tx = open.result.transaction("keyval", "readwrite");
          tx.objectStore("keyval").put(documents, "caps-documents");
          tx.oncomplete = () => (open.result.close(), resolve());
          tx.onerror = () => reject(tx.error);
        };
      });
    },
    [documents, local],
  );
}

async function readDevice(page) {
  return page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const open = indexedDB.open("keyval-store");
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const get = open.result.transaction("keyval").objectStore("keyval").get("caps-documents");
          get.onsuccess = () =>
            resolve({ documents: get.result, local: { recent: localStorage.getItem("ride-now:recent"), urls: localStorage.getItem("ride-now:live-urls") } });
          get.onerror = () => reject(get.error);
        };
      }),
  );
}

function storedWatchlist(bytes) {
  return { kinds: { watchlist: { records: { "sha256:old": { hash: "sha256:old", version: 2, bytes } }, current: "sha256:old" } } };
}

// docs/FORMATS.md, "Versions".
test("ride now boots an older device into its tagged documents, keeping the old records", async ({ page }) => {
  await seedDevice(page, {
    documents: storedWatchlist(JSON.stringify({ version: "v2", stops: [{ id: "A", label: "home" }] })),
    local: {
      "ride-now:recent": JSON.stringify([{ id: "B", name: "Main & 2nd", lat: 45.51, lon: -73.51 }]),
      "ride-now:live-urls": JSON.stringify({ trips: "https://agency.example/trip-updates.pb", vehicles: "", alerts: "" }),
    },
  });
  await open(page);

  // A watchlist from before its tags reads as v2.
  const saved = page.getByTestId("saved");
  await expect(saved).toContainText("home");
  await expect(saved).toContainText("8:02");
  await expect(page.getByTestId("recent").getByRole("button", { name: "Main & 2nd" })).toBeVisible();

  const { documents, local } = await readDevice(page);
  expect(documents.kinds.watchlist.current).toBe("sha256:old");
  expect(documents.kinds.watchlist.records["sha256:old"].bytes).toContain('"version":"v2"');
  for (const [kind, key] of [["recent_places", "ride-now:recent"], ["live_feeds", "ride-now:live-urls"]]) {
    const entry = documents.kinds[kind];
    const record = entry.records[entry.current];
    expect(JSON.parse(record.bytes)).toMatchObject({ kind, schema_version: 1 });
    expect(record.produced_by).toEqual({ op: "moved", from: `localStorage:${key}` });
  }
  expect(JSON.parse(documents.kinds.live_feeds.records[documents.kinds.live_feeds.current].bytes).trips).toBe("https://agency.example/trip-updates.pb");
  expect(local).toEqual({ recent: null, urls: null });
});

test("ride now brings an older saved-stops record forward, keeping the old one", async ({ page }) => {
  const v1 = JSON.stringify({ version: "v1", stops: [{ id: "A", note: "home", color: "red" }] });
  await seedDevice(page, { documents: storedWatchlist(v1), local: {} });
  await open(page);
  // The board refuses v1, and the migration brings it forward.
  const saved = page.getByTestId("saved");
  await expect(saved).toContainText("home");
  await expect(saved).toContainText("8:02");
  const { documents } = await readDevice(page);
  const entry = documents.kinds.watchlist;
  expect(entry.records["sha256:old"].bytes).toBe(v1);
  const current = entry.records[entry.current];
  expect(entry.current).not.toBe("sha256:old");
  expect(JSON.parse(current.bytes)).toEqual({ kind: "watchlist", schema_version: 2, stops: [{ id: "A", label: "home" }] });
  expect(current.produced_by).toEqual({ op: "migrate", from: "sha256:old", by: "migrate.migrate" });
});

test("ride now shows a detour on the board and on the trip", async ({ page }) => {
  await open(page, { edits: detour });

  await pick(page, "stop name", "main 3", "Main & 3rd");
  const board = page.getByTestId("rider-board");
  await expect(board.getByTestId("board-skipped")).toContainText("8:45");

  await board.getByRole("button", { name: "Directions to here" }).click();
  await pick(page, "From", "main 1", "Main & 1st");
  await expect(page.getByTestId("plan-notice")).toBeVisible();
});

test("ride now lists a cancelled trip as cancelled, never as a departure", async ({ page }) => {
  await open(page, { edits: cancelT10 });

  await pick(page, "stop name", "main 1s", "Main & 1st");
  const board = page.getByTestId("rider-board");
  await expect(board.getByTestId("board-cancelled")).toContainText("8:00");
  await expect(board.getByTestId("board-skipped")).toHaveCount(0);
  await expect(board.getByTestId("board-rows")).not.toContainText("8:02");
});

test("ride now refuses an edit document a newer build wrote, never reading it as its own", async ({ page }) => {
  // After a rollback a page can meet data that a newer build wrote.
  // src/versions.test.mjs covers the refusal's request for the deployment's build.
  const posts = [];
  page.on("request", (request) => {
    if (request.method() === "POST" && request.url().endsWith("/edits")) posts.push(request.url());
  });
  await open(page, { edits: detour, editsVersion: 3 });
  await pick(page, "stop name", "main 3", "Main & 3rd");
  const board = page.getByTestId("rider-board");
  await expect(board).toContainText("edit_records v3 is newer than this module, which reads v2");
  await expect(board.getByTestId("board-skipped")).toHaveCount(0);
  expect(posts).toEqual([]);
});

test("ride now degrades frame by frame when a capability fails to load", async ({ page }) => {
  // The linked assembly's failure is every capability's, so each falls back to
  // its own module. Two of those fail too.
  await page.route(/\/caps\/(ui-ride-now|board|router)-[0-9a-f]+\.wasm$/, (route) => route.abort());
  await open(page);

  await pick(page, "stop name", "main 1", "Main & 1st");
  await expect(page.getByTestId("rider-board")).toContainText("Departures are unavailable right now");

  await page.getByTestId("rider-board").getByRole("button", { name: "Directions from here" }).click();
  await pick(page, "To", "main 2", "Main & 2nd");
  await expect(page.getByTestId("trip-plan")).toContainText("Trip planning is unavailable right now");
});

test("ride now explores the routes and tracks a bus", async ({ page }) => {
  await open(page);

  await page.getByRole("button", { name: "Routes" }).click();
  const list = page.getByTestId("route-list");
  await expect(list.locator("li")).toHaveCount(3);
  await page.getByLabel("filter routes").fill("downtown");
  await expect(list.locator("li")).toHaveCount(1);
  await expect(list).toContainText("Downtown via Main");

  await list.locator("li button").first().click();
  const route = page.getByTestId("route-view");
  await expect(route.getByTestId("route-variant")).toHaveCount(2);
  await expect(route.getByTestId("route-variant").first()).toContainText("Main & 1st → Main & 2nd");
  await expect(route.getByTestId("route-variant").nth(1)).toContainText("via Main & 3rd");

  await route.getByRole("button", { name: "Main & 1st" }).first().click();
  const board = page.getByTestId("rider-board");
  await board.getByTestId("board-rows").locator("li button").first().click();
  await expect(board.getByTestId("track")).toContainText("Bus 4201");
  await board.getByRole("button", { name: "Track this bus ›" }).click();

  const bus = page.getByTestId("bus-view");
  // A bus is as late as its trip's worst stated delay (+180 s at B).
  await expect(bus).toContainText("3 min late");
  const stops = bus.getByTestId("bus-stops").locator("li");
  await expect(stops).toHaveCount(2);
  await expect(stops.first()).toContainText("Main & 1st");
  await expect(stops.first()).toContainText("8:02");
  await expect(stops.nth(1)).toContainText("Main & 2nd");
  await expect(stops.nth(1)).toContainText("8:13");
});

test("ride now says how old detours are while the server's store is not answering", async ({ page }) => {
  await open(page, { staleAsOf: new Date("2026-10-23T07:40:00-04:00").getTime() });
  await expect(page.getByTestId("ride-stale")).toContainText("Detours as of 7:40");
});
