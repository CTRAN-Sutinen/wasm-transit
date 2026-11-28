// The disruptions UI (src/uis/disruptions; D-213). The server on
// :8084 is the stops spec's, whose durable store the notes need (POST /notes).
// The time is Friday 2026-10-23 at 07:55 in America/Montreal. The edit document
// holds a detour of route 10 (C skipped) and a cancel of t30's 8:40. The trip
// updates (sample-rt.pb) serve every live feed URL.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "fixtures",
);

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
    cancel: {
      id: "cancel-t30",
      trip_id: "t30",
      start_secs: 31200,
      date: "2026-10-23",
    },
  },
];

/// The server refuses a note or an alert that names an edit it does not hold (D-233).
async function seedEdits(request) {
  const posted = await request.post("/edits", {
    data: { kind: "edit_records", schema_version: 2, records: edits },
  });
  expect(posted.ok()).toBeTruthy();
}

async function open(page, hash = "") {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await page.route("**/edits", (route) =>
    route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({
        kind: "edit_records",
        schema_version: 2,
        records: edits,
      }),
    }),
  );
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({
      contentType: "application/octet-stream",
      body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")),
    }),
  );
  await page.goto(`/disruptions.html${hash}`);
}

test("the day's disruptions: status, a note, a stop, and dispatch", async ({
  page,
  request,
}) => {
  await seedEdits(request);
  const note = await request.post("/notes", {
    data: {
      id: "n-e2e-cancel",
      target: { kind: "edit", id: "c-disruptions" },
      text: "No operator. Next trip covered.",
    },
  });
  expect(note.ok()).toBeTruthy();

  await open(page);
  await expect(page.getByTestId("live-state")).toContainText("Live", {
    timeout: 15_000,
  });

  const card = page.locator('[data-testid="route-card"][data-route="10"]');
  await expect(card).toBeVisible();
  const lines = card.getByTestId("disruption-line");
  await expect(lines).toHaveCount(2);
  await expect(lines.nth(0)).toContainText("Detour: Skips 1 stop");
  await expect(lines.nth(1)).toContainText("Cancelled: 8:40 AM from");
  await expect(lines.nth(1)).toContainText("No alert");
  await expect(lines.nth(1)).toContainText("No operator. Next trip covered.");
  await expect(page.getByTestId("route-status")).toContainText(
    /Other routes \(\d+\)/,
  );

  await lines.nth(1).click();
  const detail = page.getByTestId("disruption-detail");
  await expect(detail).toContainText("Cancelled");
  await expect(detail).toContainText("Stops not served 3");
  await expect(detail.getByTestId("detail-notes")).toContainText(
    "No operator. Next trip covered.",
  );

  await page.reload();
  await expect(page.getByTestId("disruption-detail")).toContainText(
    "Stops not served 3",
    { timeout: 15_000 },
  );
  await detail.getByRole("button", { name: "‹ Back" }).click();

  await page.getByLabel("Search routes and stops").fill("3rd");
  await page
    .getByRole("listbox")
    .getByRole("button", { name: /Main & 3rd/ })
    .click();
  await expect(page.getByTestId("search-pick")).toBeVisible();
  await expect(card.getByTestId("disruption-line")).toHaveCount(2);

  // The note above carries the server's clock, on another day, so it is not today's.
  await page
    .getByRole("navigation", { name: "Views" })
    .getByRole("button", { name: "Dispatch" })
    .click();
  const board = page.getByTestId("dispatch-board");
  await expect(board.getByTestId("late-row").first()).toContainText("Bus 4201");
  await expect(board.getByTestId("detours")).toContainText("Skips 1 stop");
  await expect(board.getByTestId("cancel-row")).toContainText("8:40 AM");
  await expect(board.getByTestId("day-notes")).toContainText("No notes today");
});

test("an alert from a gap, published and ended, and a note written", async ({
  page,
  request,
}) => {
  await seedEdits(request);
  const head = await request.get("/manifest").then((r) => r.json());
  const templates = await request.post("/alert-templates", {
    data: {
      from: head.alert_templates_hash ?? "",
      templates: {
        kind: "alert_templates",
        schema_version: 1,
        templates: {
          "trip-cancelled": {
            name: "Trip cancelled",
            cause: "other",
            effect: "no_service",
            severity: "warning",
            header: [{ text: "Route {route}: trip cancelled on {start date}" }],
          },
        },
      },
    },
  });
  expect(templates.ok()).toBeTruthy();

  await open(page, "#view=alerts");
  const gaps = page.getByTestId("gaps");
  await expect(gaps.getByTestId("start-alert")).toHaveCount(2, {
    timeout: 15_000,
  });

  await gaps
    .locator("div.row", { hasText: "8:40 AM trip cancelled" })
    .getByTestId("start-alert")
    .click();
  const editor = page.getByTestId("alert-editor");
  await expect(editor.getByTestId("applies-to")).toContainText(
    "Route 10 · 8:40 AM trip",
  );
  await editor.getByTestId("template").selectOption("trip-cancelled");
  await expect(editor.getByTestId("header")).toHaveValue(
    "Route 10: trip cancelled on Friday, October 23",
  );
  const card = page.getByTestId("reach-card");
  await expect(card).toHaveCount(1);
  await expect(card).toContainText(
    'route_id: "10", trip { trip_id: "t30", start_date: "20261023", start_time: "08:40:00" }',
  );
  await expect(card).toContainText("2 departures at 2 stops today");
  await editor.getByTestId("open-preview").click();
  await expect(page.getByTestId("preview-dialog")).toContainText("Main & 1st");
  await page
    .getByTestId("preview-dialog")
    .getByRole("button", { name: "Close" })
    .click();
  await editor.getByTestId("publish").click();
  await expect(page.getByTestId("alert-row")).toContainText(
    "Route 10: trip cancelled on Friday, October 23",
  );
  await expect(gaps.getByTestId("start-alert")).toHaveCount(1);

  await page.clock.setFixedTime(new Date("2026-10-23T08:05:00-04:00"));
  await page.getByTestId("end-alert").click();
  await page.getByRole("tab", { name: /Ended/ }).click();
  await expect(page.getByTestId("alert-row")).toContainText(
    "Route 10: trip cancelled",
  );
  await expect(gaps.getByTestId("start-alert")).toHaveCount(2);

  await page
    .getByRole("navigation", { name: "Views" })
    .getByRole("button", { name: "Status" })
    .click();
  await page
    .locator('[data-testid="route-card"][data-route="10"]')
    .getByTestId("disruption-line")
    .first()
    .click();
  const detail = page.getByTestId("disruption-detail");
  await detail
    .getByTestId("note-box")
    .getByLabel("Note")
    .fill("Water main break. Crew says 3 hours.");
  await detail.getByTestId("add-note").click();
  await expect(detail.getByTestId("detail-notes")).toContainText(
    "Water main break. Crew says 3 hours.",
  );
  await detail
    .getByTestId("detail-notes")
    .getByRole("button", { name: "Edit" })
    .click();
  await detail
    .getByTestId("detail-notes")
    .getByRole("textbox")
    .fill("Water main break. Crew says 4 hours.");
  await detail
    .getByTestId("detail-notes")
    .getByRole("button", { name: "Save" })
    .click();
  await expect(detail.getByTestId("detail-notes")).toContainText(
    "Crew says 4 hours.",
  );
  await expect(detail.getByTestId("detail-notes")).toContainText("Edited");
});
