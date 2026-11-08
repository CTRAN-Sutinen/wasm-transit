// The stop display (src/uis/stop-display) — a generated UI for the
// screen at a stop (docs/ARCHITECTURE.md, web section, "Generated
// UIs"), in three full-screen layouts over one answer.
//
// Friday 2026-10-23 at 07:55 at the agency (the fixture's America/
// Montreal; the browser in UTC, which never decides), the fixture feed,
// and its trip updates (sample-rt.pb: t10 +120 s at A — served for
// every live feed URL):
//
//   1. The setup picks a layout and a stop (directory `stops`); the
//      URL keeps both. Several routes at A (board `grouped_board`):
//      10 → Downtown leaves in 7 min, 2 min late, live, then 45 min;
//      20 → Downtown in 25 min, scheduled. The alerts slot says none.
//   2. A transit center opened from its URL: the station pin pools its
//      bays, each time naming the bay it leaves from.
//   3. A single route: a detour in the edit document takes the 10 from
//      C, and its group says so; a cancel of the 08:00 10 (t10) takes
//      it out of the group's times and the group says a trip was
//      cancelled (D-45, D-100); after the day's last departure at A,
//      its groups stay and say when each next runs (board `next`, D-42).
//
// The live feeds and the edit document are served by the test (the
// shared e2e server holds other specs' edits).

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

test.use({ timezoneId: "UTC" });

const detour = [
  {
    id: "e-stop-display",
    rev: 1,
    status: "accepted",
    updated_at: 1,
    diversion: {
      id: "d-stop-display",
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

const cancel = [
  {
    id: "c-stop-display",
    rev: 1,
    status: "accepted",
    updated_at: 1,
    cancel: { id: "cancel-t10", trip_id: "t10", start_secs: null, date: "2026-10-23" },
  },
];

async function open(page, { query = "", at = "2026-10-23T07:55:00-04:00", edits = [] } = {}) {
  await page.clock.setFixedTime(new Date(at));
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ kind: "edit_records", schema_version: 2, records: edits }) }));
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
  await page.goto(`/stop-display.html${query}`);
  await page.getByLabel("Agency GTFS zip").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
}

/// One group on the display, by its route and its destination exactly.
const group = (page, route, destination) =>
  page
    .locator(`[data-testid="display-group"][data-route="${route}"]`)
    .filter({ has: page.getByTestId("display-dest").getByText(destination, { exact: true }) });

test("the stop display is set up and shows several routes", async ({ page }) => {
  await open(page);

  // 1. The setup: a layout, then the stop by name.
  await expect(page.getByTestId("display-setup")).toBeVisible({ timeout: 10_000 });
  await page.getByTestId("layout-routes").click();
  const input = page.getByLabel("Stop", { exact: true });
  await input.fill("Main & 1st");
  await expect(page.getByRole("option").first()).toContainText("Main & 1st");
  await input.press("Enter");

  await expect(page.getByTestId("display")).toHaveAttribute("data-layout", "routes");
  await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st");
  await expect(page).toHaveURL(/stop=A/);
  await expect(page).toHaveURL(/layout=routes/);
  await expect(page.getByTestId("display-live")).toContainText("Live");

  const ten = group(page, "10", "Downtown");
  await expect(ten.getByTestId("display-time")).toHaveCount(2);
  await expect(ten.getByTestId("display-time").first()).toContainText("7 min");
  await expect(ten.getByTestId("display-time").first()).toContainText("2 min late");
  await expect(ten.getByTestId("display-time").nth(1)).toContainText("45 min");
  const twenty = group(page, "20", "Downtown");
  await expect(twenty.getByTestId("display-time").first()).toContainText("25 min");
  await expect(twenty.getByTestId("display-time").first()).toContainText("Scheduled");
  await expect(page.getByTestId("display-alerts")).toContainText("No service alerts");
});

test("a transit center names the bay each bus leaves from", async ({ page }) => {
  await open(page, { query: "?stop=STN&layout=center" });

  await expect(page.getByTestId("display-stop")).toHaveText("Main Station", { timeout: 10_000 });
  await expect(page.getByTestId("display")).toHaveAttribute("data-layout", "center");
  const toBayA = group(page, "40", "Bay A");
  await expect(toBayA.getByTestId("display-time").first()).toContainText("40 min");
  await expect(toBayA.getByTestId("display-time").first()).toContainText("Main Station: Bay B");
  const toBayB = group(page, "40", "Bay B");
  await expect(toBayB.getByTestId("display-time").first()).toContainText("Main Station: Bay A");
});

// A screen's URL is persisted data (docs/FORMATS.md, "Versions"): one
// configured by a newer build, with a parameter this build does not
// know, loads its stop and keeps the parameter.
test("a display URL with a parameter this build does not know loads", async ({ page }) => {
  await open(page, { query: "?stop=A&layout=routes&brightness=night" });
  await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 10_000 });
  await expect(page.getByTestId("display")).toHaveAttribute("data-layout", "routes");
  await expect(page).toHaveURL(/brightness=night/);
});

test("a single route says when the edits detour it away", async ({ page }) => {
  await open(page, { query: "?stop=C&layout=route", edits: detour });
  await expect(page.getByTestId("display-stop")).toHaveText("Main & 3rd", { timeout: 10_000 });
  await expect(group(page, "10", "Downtown").getByTestId("display-none")).toHaveText("Detoured — not serving this stop");
});

test("a cancelled trip leaves the group's times, and the group says so", async ({ page }) => {
  await open(page, { query: "?stop=A&layout=route", edits: cancel });
  await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 10_000 });
  const ten = group(page, "10", "Downtown");
  await expect(ten.getByTestId("display-time")).toHaveCount(1);
  await expect(ten.getByTestId("display-time").first()).not.toContainText("7 min");
  await expect(ten.getByTestId("display-cancelled")).toHaveText("A trip was cancelled");
});

test("a route that has stopped for the day stays on the display", async ({ page }) => {
  // Friday night: the 10 next runs Sunday (t11), the 20 Monday (t20) —
  // the board's `next` (D-42), the display only formats it.
  await open(page, { query: "?stop=A&layout=route", at: "2026-10-23T23:00:00-04:00" });
  await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 10_000 });
  await expect(group(page, "10", "Downtown").getByTestId("display-none")).toHaveText("Next: Sunday 9:00 am");
  await expect(group(page, "20", "Downtown").getByTestId("display-none")).toHaveText("Next: Monday 8:20 am");
});

// 4. A display left running (docs/ARCHITECTURE.md, "The bus worker"): the agency
//    re-mints its live feeds every poll, so each poll here serves a new
//    snapshot (the fixture plus an unknown field carrying the poll's
//    number — the decode ignores it, its hash does not). The rt module
//    keeps its last few snapshots and its linear memory plateaus; a
//    module forced past its memory ceiling is replaced by a fresh
//    instance that answers the same bytes for the same request.
test("a display left polling stays bounded, and a module past its ceiling answers the same", async ({ page }) => {
  const rt = fs.readFileSync(path.join(fixtures, "sample-rt.pb"));
  let polls = 0;
  /// Field 1000, varint: the tag (1000 << 3 = 8000) and the value.
  const varint = (n) => {
    const out = [];
    do {
      out.push((n & 0x7f) | (n > 0x7f ? 0x80 : 0));
      n >>>= 7;
    } while (n > 0);
    return Buffer.from(out);
  };
  await page.clock.install({ time: new Date("2026-10-23T07:55:00-04:00") });
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: '{"kind":"edit_records","schema_version":2,"records":[]}' }));
  await page.route(/\/rt\?url=/, (route) => {
    polls += 1;
    route.fulfill({ contentType: "application/octet-stream", body: Buffer.concat([rt, varint(8000), varint(polls)]) });
  });
  await page.goto("/stop-display.html?stop=A&layout=routes");
  await page.getByLabel("Agency GTFS zip").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  const ten = group(page, "10", "Downtown");
  await expect(ten.getByTestId("display-time").first()).toContainText("2 min late", { timeout: 10_000 });

  const bus = (fn, ...args) => page.evaluate(([fn, args]) => window.__bus[fn](...args), [fn, args]);
  const snapshots = async () => (await bus("held", "rt")).held.filter((e) => e.kind === "rt snapshot").length;
  /// One poll: the display's interval fires and both feeds are fetched.
  const poll = async () => {
    const before = polls;
    await page.clock.fastForward(20_000);
    await expect.poll(() => polls).toBeGreaterThanOrEqual(before + 2);
    await expect(page.getByTestId("display-live")).toContainText("Live");
  };

  for (let i = 0; i < 15; i++) await poll();
  const half = await bus("memory", "rt");
  expect(await snapshots()).toBeLessThanOrEqual(8);
  for (let i = 0; i < 15; i++) await poll();
  const full = await bus("memory", "rt");
  expect(polls).toBeGreaterThanOrEqual(60);
  expect(await snapshots()).toBeLessThanOrEqual(8);
  expect(full.memory, "the rt module's linear memory plateaus").toBe(half.memory);
  // Ten minutes on, the late 10 has left: the display moved with the clock.
  await expect(ten.getByTestId("display-time").first()).toContainText("Scheduled");

  // The ceiling: forced below the board's working set, the next op
  // finds its instance past it — replaced, the documents registered
  // again, the same answer — once per op, never in a loop.
  const answer = await bus("replay", "board");
  await bus("ceiling", 1, 0);
  expect(await bus("replay", "board")).toBe(answer);
  expect((await bus("memory", "board")).resets).toBe(1);
  expect(await bus("replay", "board")).toBe(answer);
  expect((await bus("memory", "board")).resets).toBe(2);
  const held = (await bus("held", "board")).held.map((e) => e.kind);
  expect(held).toContain("network");
});

test("the display runs one linked module, holding the network once", async ({ page }) => {
  await open(page, { query: "?stop=A&layout=routes" });
  await expect(group(page, "10", "Downtown").getByTestId("display-time").first()).toBeVisible({ timeout: 10_000 });
  const bus = (fn, ...args) => page.evaluate(([fn, args]) => window.__bus[fn](...args), [fn, args]);
  // Every capability the display asks answers in its linked assembly (D-77).
  for (const cap of await bus("caps")) expect(await bus("module", cap)).toBe("ui:stop-display");
  // One registry: the network the board, the directory and rt all read
  // is held once.
  const networks = (await bus("held", "board")).held.filter((e) => e.kind === "network");
  expect(networks).toHaveLength(1);
});

// The deployment (docs/units/deploy-rider-apps.md): a display reads
// the AGENCY's clock (the zip's zone, as the manifest names it for a
// served network), stops polling while hidden, and when a live feed
// stops answering it keeps its board, read from the schedule and
// saying so.
test.describe("a display set to another zone", () => {
  test.use({ timezoneId: "Asia/Tokyo" });
  test("reads the agency's clock, never the device's", async ({ page }) => {
    // 07:55 in Montreal is 20:55 in Tokyo: the board and the header
    // answer Montreal's 07:55.
    await open(page, { query: "?stop=A&layout=routes" });
    const ten = group(page, "10", "Downtown");
    await expect(ten.getByTestId("display-time").first()).toContainText("7 min", { timeout: 10_000 });
    await expect(page.getByTestId("display-clock")).toContainText("7:55");
  });
});

test("a hidden display makes no requests, and re-reads on its return", async ({ page }) => {
  let requests = 0;
  await page.clock.install({ time: new Date("2026-10-23T07:55:00-04:00") });
  await page.route("**/edits", (route) => {
    requests += 1;
    route.fulfill({ contentType: "application/json", body: '{"kind":"edit_records","schema_version":2,"records":[]}' });
  });
  await page.route(/\/rt\?url=/, (route) => {
    requests += 1;
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) });
  });
  await page.goto("/stop-display.html?stop=A&layout=routes");
  await page.getByLabel("Agency GTFS zip").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  await expect(group(page, "10", "Downtown").getByTestId("display-time").first()).toContainText("2 min late", { timeout: 10_000 });

  const visibility = (hidden) =>
    page.evaluate((hidden) => {
      Object.defineProperty(document, "hidden", { configurable: true, get: () => hidden });
      document.dispatchEvent(new Event("visibilitychange"));
    }, hidden);
  await visibility(true);
  const before = requests;
  await page.clock.fastForward(5 * 20_000);
  expect(requests, "a hidden page polls nothing").toBe(before);
  await visibility(false);
  await expect.poll(() => requests).toBeGreaterThan(before);
});

test("a live feed that stops answering turns the board to the schedule, and the board stays", async ({ page }) => {
  let down = false;
  await page.clock.install({ time: new Date("2026-10-23T07:55:00-04:00") });
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: '{"kind":"edit_records","schema_version":2,"records":[]}' }));
  await page.route(/\/rt\?url=/, (route) =>
    down
      ? route.fulfill({ status: 502, body: "upstream down" })
      : route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
  await page.goto("/stop-display.html?stop=A&layout=routes");
  await page.getByLabel("Agency GTFS zip").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  const ten = group(page, "10", "Downtown");
  await expect(ten.getByTestId("display-time").first()).toContainText("2 min late", { timeout: 10_000 });
  await expect(page.getByTestId("display-live")).toContainText("Live");

  down = true;
  await page.clock.fastForward(20_000);
  // Never a stale prediction as live: the schedule, said so; the
  // groups stay on screen.
  await expect(page.getByTestId("display-live")).toContainText("Scheduled times");
  await expect(ten.getByTestId("display-time").first()).toContainText("Scheduled");
  await expect(group(page, "20", "Downtown").getByTestId("display-time").first()).toBeVisible();

  down = false;
  await page.clock.fastForward(20_000);
  await expect(page.getByTestId("display-live")).toContainText("Live");
});

// D-119: while the sync server's store does not answer, it serves the
// edit document it holds, labelled with when the store last confirmed
// it (X-Store-Stale, X-Store-As-Of); the display keeps the detour on the
// board and says how old it is, and the label goes once the store
// answers again.
test("a detour served while the server's store is not answering says its age", async ({ page }) => {
  let stale = true;
  const asOf = new Date("2026-10-23T07:40:00-04:00").getTime();
  await page.clock.install({ time: new Date("2026-10-23T07:55:00-04:00") });
  await page.route("**/edits", (route) =>
    route.fulfill({
      contentType: "application/json",
      headers: stale ? { "X-Store-Stale": "true", "X-Store-As-Of": String(asOf) } : {},
      body: JSON.stringify({ kind: "edit_records", schema_version: 2, records: detour }),
    }),
  );
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
  await page.goto("/stop-display.html?stop=A&layout=routes");
  await page.getByLabel("Agency GTFS zip").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));

  await expect(page.getByTestId("display-stale")).toContainText("Detours as of 7:40", { timeout: 10_000 });
  await expect(group(page, "10", "Downtown").getByTestId("display-time").first()).toBeVisible();

  stale = false;
  await page.clock.fastForward(20_000);
  await expect(page.getByTestId("display-stale")).toHaveCount(0);
});
