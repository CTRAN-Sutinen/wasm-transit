// The disruptions UI (src/uis/disruptions; D-213, D-214, D-218): what is
// disrupted today, and does an alert explain it. Its server is the
// stops spec's (playwright.config.mjs, :8084): the durable store, which
// the notes need (POST /notes).
//
// Friday 2026-10-23 at 07:55 at the agency (the fixture's America/
// Montreal; the browser in UTC, which never decides), the fixture feed,
// an edit document with a detour of route 10 (C skipped) and a cancel
// of t30's 8:40, and the trip updates (sample-rt.pb: t10 three minutes
// late at B, bus 4201 near A, served for every live feed URL):
//
//   1. Status lists route 10 as a card: the detour and the cancel, each
//      with "No alert", the cancel with its first stop. The other
//      routes are counted.
//   2. A note written about the cancel (POST /notes, the server's
//      stamp) shows as its latest note, and in the cancel's own panel,
//      with its stops not served and its path on the map.
//   3. The search narrows the page to one stop: only what touches C.
//   4. Dispatch lists t10 late now with its bus, the detour, and the
//      cancel. Today's notes are the page's day's: the note above is
//      stamped by the server's clock, on another day, so it is not
//      among them.
//   5. The view, the pick, and the open disruption live in the URL
//      hash: a reload opens the same.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

test.use({ baseURL: "http://127.0.0.1:8084", timezoneId: "UTC" });

const edits = [
  {
    id: "e-disruptions",
    rev: 1,
    status: "accepted",
    updated_at: 1,
    diversion: {
      id: "d-disruptions",
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
  {
    id: "c-disruptions",
    rev: 1,
    status: "accepted",
    updated_at: 1,
    cancel: { id: "cancel-t30", trip_id: "t30", start_secs: 31200, date: "2026-10-23" },
  },
];

/// The edits on the server too: a note or an alert that names an edit
/// the server does not hold is refused (D-233). The page reads them
/// through `open`'s route, as of the spec's fixed time.
async function seedEdits(request) {
  const posted = await request.post("/edits", { data: { kind: "edit_records", schema_version: 2, records: edits } });
  expect(posted.ok()).toBeTruthy();
}

async function open(page, hash = "") {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await page.route("**/edits", (route) =>
    route.fulfill({ contentType: "application/json", body: JSON.stringify({ kind: "edit_records", schema_version: 2, records: edits }) }),
  );
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
  await page.goto(`/disruptions.html${hash}`);
}

test("the day's disruptions: status, a note, a stop, and dispatch", async ({ page, request }) => {
  await seedEdits(request);
  // 2. A note about the cancel, written through the receive path.
  const note = await request.post("/notes", {
    data: { id: "n-e2e-cancel", target: { kind: "edit", id: "c-disruptions" }, text: "No operator. Next trip covered." },
  });
  expect(note.ok()).toBeTruthy();

  await open(page);
  await expect(page.getByTestId("live-state")).toContainText("Live", { timeout: 15_000 });

  // 1. Route 10's card: the detour and the cancel, neither explained.
  const card = page.locator('[data-testid="route-card"][data-route="10"]');
  await expect(card).toBeVisible();
  const lines = card.getByTestId("disruption-line");
  await expect(lines).toHaveCount(2);
  await expect(lines.nth(0)).toContainText("Detour: Skips 1 stop");
  await expect(lines.nth(1)).toContainText("Cancelled: 8:40 AM from");
  await expect(lines.nth(1)).toContainText("No alert");
  await expect(lines.nth(1)).toContainText("No operator. Next trip covered.");
  await expect(page.getByTestId("route-status")).toContainText(/Other routes \(\d+\)/);

  // 2. The cancel's own panel: its stops not served, its notes.
  await lines.nth(1).click();
  const detail = page.getByTestId("disruption-detail");
  await expect(detail).toContainText("Cancelled");
  await expect(detail).toContainText("Stops not served 3");
  await expect(detail.getByTestId("detail-notes")).toContainText("No operator. Next trip covered.");

  // 5. A reload opens the same panel.
  await page.reload();
  await expect(page.getByTestId("disruption-detail")).toContainText("Stops not served 3", { timeout: 15_000 });
  await detail.getByRole("button", { name: "‹ Back" }).click();

  // 3. One stop: only what touches C (the detour skips it; t30 serves it).
  await page.getByLabel("Search routes and stops").fill("3rd");
  await page.getByRole("listbox").getByRole("button", { name: /Main & 3rd/ }).click();
  await expect(page.getByTestId("search-pick")).toBeVisible();
  await expect(card.getByTestId("disruption-line")).toHaveCount(2);

  // 4. Dispatch: t10 late, the detour, the cancel; no note on the day.
  await page.getByRole("navigation", { name: "Views" }).getByRole("button", { name: "Dispatch" }).click();
  const board = page.getByTestId("dispatch-board");
  await expect(board.getByTestId("late-row").first()).toContainText("Bus 4201");
  await expect(board.getByTestId("detours")).toContainText("Skips 1 stop");
  await expect(board.getByTestId("cancel-row")).toContainText("8:40 AM");
  await expect(board.getByTestId("day-notes")).toContainText("No notes today");
});

// The writing side, on the same day:
//
//   6. A template (POST /alert-templates) fills the alert started from
//      the cancel's gap: its route's name and the day. The alert is
//      published (POST /service-alerts) and leaves the gap list; in
//      force, it explains the cancel on Status.
//   7. What riders see: the cancel's one entity as the feed publishes
//      it (route 10, trip t30's 8:40 instance) and the departures it
//      reaches today (t30 at A and C). The preview, opened from its
//      button, is at the trip's first stop.
//   8. Ended, it is listed as ended, and the cancel is a gap again.
//   9. A note is added on the detour's panel and then edited.
test("an alert from a gap, published and ended, and a note written", async ({ page, request }) => {
  await seedEdits(request);
  const head = await request.get("/manifest").then((r) => r.json());
  const templates = await request.post("/alert-templates", {
    data: {
      from: head.alert_templates_hash ?? "",
      templates: {
        kind: "alert_templates",
        schema_version: 1,
        templates: { "trip-cancelled": { name: "Trip cancelled", cause: "other", effect: "no_service", severity: "warning", header: [{ text: "Route {route}: trip cancelled on {start date}" }] } },
      },
    },
  });
  expect(templates.ok()).toBeTruthy();

  await open(page, "#view=alerts");
  const gaps = page.getByTestId("gaps");
  await expect(gaps.getByTestId("start-alert")).toHaveCount(2, { timeout: 15_000 });

  // 6. Started from the cancel's gap, filled, and published.
  await gaps.locator("div.row", { hasText: "8:40 AM trip cancelled" }).getByTestId("start-alert").click();
  const editor = page.getByTestId("alert-editor");
  await expect(editor.getByTestId("applies-to")).toContainText("Route 10 · 8:40 AM trip");
  await editor.getByTestId("template").selectOption("trip-cancelled");
  await expect(editor.getByTestId("header")).toHaveValue("Route 10: trip cancelled on Friday, October 23");
  // 7. What riders see, and the preview at the trip's first stop.
  const card = page.getByTestId("reach-card");
  await expect(card).toHaveCount(1);
  await expect(card).toContainText('route_id: "10", trip { trip_id: "t30", start_date: "20261023", start_time: "08:40:00" }');
  await expect(card).toContainText("2 departures at 2 stops today");
  await editor.getByTestId("open-preview").click();
  await expect(page.getByTestId("preview-dialog")).toContainText("Main & 1st");
  await page.getByTestId("preview-dialog").getByRole("button", { name: "Close" }).click();
  await editor.getByTestId("publish").click();
  await expect(page.getByTestId("alert-row")).toContainText("Route 10: trip cancelled on Friday, October 23");
  await expect(gaps.getByTestId("start-alert")).toHaveCount(1);

  // 8. Ended ten minutes later: listed as ended, and a gap again.
  await page.clock.setFixedTime(new Date("2026-10-23T08:05:00-04:00"));
  await page.getByTestId("end-alert").click();
  await page.getByRole("tab", { name: /Ended/ }).click();
  await expect(page.getByTestId("alert-row")).toContainText("Route 10: trip cancelled");
  await expect(gaps.getByTestId("start-alert")).toHaveCount(2);

  // 9. A note on the detour, then edited.
  await page.getByRole("navigation", { name: "Views" }).getByRole("button", { name: "Status" }).click();
  await page.locator('[data-testid="route-card"][data-route="10"]').getByTestId("disruption-line").first().click();
  const detail = page.getByTestId("disruption-detail");
  await detail.getByTestId("note-box").getByLabel("Note").fill("Water main break. Crew says 3 hours.");
  await detail.getByTestId("add-note").click();
  await expect(detail.getByTestId("detail-notes")).toContainText("Water main break. Crew says 3 hours.");
  await detail.getByTestId("detail-notes").getByRole("button", { name: "Edit" }).click();
  await detail.getByTestId("detail-notes").getByRole("textbox").fill("Water main break. Crew says 4 hours.");
  await detail.getByTestId("detail-notes").getByRole("button", { name: "Save" }).click();
  await expect(detail.getByTestId("detail-notes")).toContainText("Crew says 4 hours.");
  await expect(detail.getByTestId("detail-notes")).toContainText("Edited");
});
