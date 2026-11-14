// The stops UI (src/uis/stops, D-157): every stop's own life, apart from
// the service plans. Its own server (playwright.config.mjs, :8084) runs
// the durable store, which a stops write needs (POST /stops); no stops
// document is kept until the first change, which saves the stops of the
// network running today (cap-stops `seed`).
//
// The page is full-width screens: the stops (a map beside their list,
// or a table), the work (a table), the feed's review, a stop's page, a
// request's page, and a survey's page; the map takes over only to place
// a point (a new stop, a move). A short change is a card on the stop's
// page, its refusals beside its actions.
//
//   - a stop in service is read on a date, with the routes that stop
//     there in the schedule running that day;
//   - suspended for dates (a refused end before its start first, said
//     beside Save), it reads suspended on a day inside them, counted and
//     listed so;
//   - it returns early, the return's date corrected — the suspension
//     then ends on the return, not its planned end — and it moves —
//     the map's click fills the point's fields, and a measured point
//     typed over it is the one saved, to the digit;
//   - details saved with nothing changed say so; the map's key names the
//     open stop only while one is open; Back leaves a task;
//   - a new stop is planned at a point typed (an id taken refused beside
//     the form), goes straight on to its installation request, then is
//     permitted and retired — hidden from the map until retired stops
//     are shown;
//   - a stop is surveyed (D-158) on one page of headed sections: the
//     half-filled survey survives a reload on this device, a count that
//     is not a number is refused beside Save, the summary lists what
//     changed, the surveyor is the person writing (the
//     server's, never typed), a photo is added and named by its hash,
//     and the saved survey shows on the Site tab grouped as the
//     worksheet groups it; on a phone the survey is the whole screen;
//   - a request for work at a stop (D-159) is made on its page,
//     submitted, its utility locate added, its work order opened,
//     completed, and the stop change that carries it out names it,
//     recorded from the completed request; a field report saves a draft
//     request under an id of its own, and a request is cancelled after
//     a confirmation;
//   - the work order system through its adapter (D-165; the stub on this
//     server): a submitted request's work order opened, then its status
//     read — the answers written on the request;
//   - the work across every stop (cap-requests `list`): filtered by
//     status and job, a row opening its request on its stop;
//   - the stops near the device (cap-stops `stops_near`), nearest first;
//   - the stops' map is kept, not rebuilt, on the way to a stop and back;
//   - signed in (D-58, D-163; its own server on :8090 with sign-in on,
//     and a stand-in provider on :8089): a write before signing in is
//     refused in words and its draft kept; signing in goes out to the
//     provider and back to the same stop; the survey then saves, its
//     surveyor the signed-in person's name, and a photo added and shown
//     only to the signed-in;
//   - what the agency's feed did to the stops record (D-150: the feed
//     applies, and the planner sees what it did): the Feed review page,
//     each stop linked to its record.

import { test, expect } from "@playwright/test";

/// A 1x1 PNG: a photo as small as a test needs.
const PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==", "base64");

test.use({ baseURL: "http://127.0.0.1:8084" });

/// A change chosen from the stop's "Record change" list.
async function change(page, kind) {
  await page.getByTestId("action-change").click();
  await page.getByTestId(`action-${kind}`).click();
  await expect(page.getByTestId("change")).toBeVisible();
}

/// The map in view (the stops' map is kept, hidden, under other screens).
const shownMap = (page) => page.locator('[data-testid="stop-life-map"]:visible');
/// The map in view, loaded (a click before then places nothing).
const readyMap = (page) => page.locator('[data-testid="stop-life-map"][data-ready="true"]:visible');

test("a stop's life: suspended and back, moved, and a new stop planned, requested, permitted, retired", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=B");
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("stop-status")).toContainText("In service");
  await expect(page.getByTestId("stop-routes")).not.toContainText("No routes");

  // A suspension whose end comes before its start is refused, beside Save.
  await change(page, "suspended");
  await page.getByTestId("form-on").fill("2026-11-09");
  await page.getByTestId("form-temporary").click();
  await page.getByTestId("form-until").fill("2026-11-05");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("change-problems")).toContainText("the end must come after the first day", { timeout: 15_000 });

  // Suspended Monday 11-09 until Thursday 11-12, for paving.
  await page.getByTestId("form-until").fill("2026-11-12");
  await page.getByTestId("form-note").fill("Paving");
  await expect(page.getByTestId("change-summary")).toContainText("Suspend · 2026-11-09 – 2026-11-12");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("status")).toContainText("Suspended Main & 2nd from 2026-11-09 until 2026-11-12", { timeout: 15_000 });

  await page.getByTestId("stops-on").fill("2026-11-10");
  await expect(page.getByTestId("stop-status")).toContainText("Suspended");
  await expect(page.getByTestId("stop-status")).toContainText("since 2026-11-09, until 2026-11-12");
  await expect(page.getByTestId("stop-status")).toContainText("Paving");
  await expect(page.locator('[data-testid="map-key-suspended"]:visible')).toBeVisible();

  // Back early on Wednesday — entered as Thursday, then corrected.
  await change(page, "in_service");
  await page.getByTestId("form-on").fill("2026-11-10");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-status")).toContainText("since 2026-11-10", { timeout: 15_000 });
  const events = page.getByTestId("stop-event");
  // newest first: the return is the first row
  await expect(events.first()).toContainText("In service");
  await events.first().getByRole("button", { name: "Correct" }).click();
  await page.getByTestId("form-on").fill("2026-11-11");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("status")).toContainText("Corrected the in service event on Main & 2nd", { timeout: 15_000 });
  await expect(events.first()).toContainText("2026-11-11");
  await expect(page.getByTestId("stop-status")).toContainText("Suspended", { timeout: 10_000 });
  // back early: the suspension ends on the return, not on its planned end
  await expect(page.getByTestId("stop-status")).toContainText("until 2026-11-11");

  // Moved: a click on the map fills the point's fields; the measured
  // point typed over it is the one saved, to the digit.
  await page.getByTestId("stops-on").fill("2026-11-20");
  await expect(page.getByTestId("stop-status")).toContainText("In service");
  await change(page, "move");
  await expect(page.getByTestId("form-save")).toBeDisabled();
  await expect(page.locator('[data-testid="map-placing"]:visible')).toBeVisible();
  await readyMap(page).locator("canvas").click({ position: { x: 300, y: 300 } });
  await expect(page.getByTestId("move-point-lat")).toHaveValue(/^-?\d+(\.\d+)?$/);
  await expect(page.getByTestId("move-point-lon")).toHaveValue(/^-?\d+(\.\d+)?$/);
  await page.getByTestId("move-point-lat").fill("45.5123456");
  await page.getByTestId("move-point-lon").fill("-73.5123456");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("status")).toContainText("Moved Main & 2nd from 2026-11-20", { timeout: 15_000 });
  await expect(events.first()).toContainText("to 45.5123456, -73.5123456");
  await expect(page.getByTestId("stop-location")).toHaveText("45.5123456, -73.5123456");
  // details saved with nothing changed say so, and the task stays open
  await change(page, "details");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("form-unchanged")).toBeVisible();
  await expect(page.getByTestId("change")).toBeVisible();
  // the browser's Back leaves a task
  await page.goBack();
  await expect(page.getByTestId("change")).toHaveCount(0);
  await expect(page.getByTestId("stop-card")).toBeVisible();

  // A new stop, planned at a point typed (no map click); an id taken is
  // refused beside the form; then straight on to its installation request.
  await page.getByTestId("new-stop-start").click();
  await expect(page.getByTestId("new-next")).toBeDisabled();
  await page.getByTestId("new-point-lat").fill("45.5051234");
  await page.getByTestId("new-point-lon").fill("-73.5051234");
  await page.getByTestId("new-next").click();
  await page.getByTestId("new-id").fill("B");
  await page.getByTestId("new-name").fill("Main & 9th");
  await page.getByTestId("new-save").click();
  await expect(page.getByTestId("new-stop-problems")).toContainText("There is already a stop B");
  await page.getByTestId("new-id").fill("N1");
  await page.getByTestId("new-save").click();
  await expect(page.getByTestId("status")).toContainText("Added stop N1", { timeout: 15_000 });
  const flow = page.getByTestId("request-view");
  await expect(flow).toBeVisible();
  await expect(page.getByTestId("request-job-new")).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("request-id")).toHaveValue("N1-20261120");
  await page.getByTestId("request-save-draft").click();
  await expect(page.getByTestId("request-track-drafted")).toHaveClass(/now/, { timeout: 15_000 });
  await expect(page.getByTestId("request-view")).toContainText("New stop");
  await page.getByTestId("request-back").click();

  await expect(page.getByTestId("stop-status")).toContainText("Planned");
  await expect(page.getByTestId("stop-status")).toContainText("since 2026-11-20");
  await expect(page.getByTestId("stop-location")).toHaveText("45.5051234, -73.5051234");
  await page.getByTestId("action-change").click();
  await expect(page.getByTestId("action-in_service")).toBeVisible();
  await expect(page.getByTestId("action-suspended")).toHaveCount(0);
  await page.getByTestId("action-permitted").click();
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-status")).toContainText("Permitted", { timeout: 15_000 });

  // Retired: the stop leaves the map and the list until retired stops
  // are shown.
  await change(page, "retired");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-status")).toContainText("Retired", { timeout: 15_000 });
  await page.getByTestId("stop-close").click();
  // the map's key names the open stop only while one is open
  await expect(page.locator('[data-testid="map-key-open"]:visible')).toHaveCount(0);
  await expect(page.getByTestId("filter-retired")).toContainText("1");
  const rows = page.getByTestId("stop-row");
  await expect(rows.filter({ hasText: "Main & 9th" })).toHaveCount(0);
  await page.getByTestId("filter-retired").click();
  await expect(rows.filter({ hasText: "Main & 9th" })).toHaveCount(1);
});

test("a stop's site survey: one page of sections, kept as a draft, checked, summarised, saved, and shown", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=A");
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 15_000 });
  // what the schedule says beside the survey: the stop beside it, and how far
  await expect(page.getByTestId("stop-visits")).toContainText(/(before|after) Main & 2nd \(~?\d+ m\)/);
  await expect(page.getByTestId("stop-surveyed")).toContainText("Never surveyed.");

  // Half filled, then the tab is reloaded: the survey is back.
  await page.getByTestId("action-survey").click();
  await expect(page.getByTestId("survey-nav-s-location")).toBeVisible();
  await page.getByTestId("survey-on_street").fill("Main St");
  await page.getByTestId("survey-position-far_side").click();
  await page.reload();
  await expect(page.getByTestId("survey-restored")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("survey-on_street")).toHaveValue("Main St");
  await expect(page.getByTestId("survey-position-far_side")).toHaveAttribute("aria-pressed", "true");
  // the index counts each section's answers and marks a change
  await expect(page.getByTestId("survey-nav-s-location")).toContainText("2/");
  // left, the stop says a draft is kept
  await page.getByTestId("survey-close").click();
  await expect(page.getByTestId("action-survey")).toHaveText("Continue survey");
  await page.getByTestId("action-survey").click();

  // A count that is not a whole number is refused beside Save, and
  // nothing is lost.
  await page.getByTestId("survey-nav-s-street").click();
  await page.getByTestId("survey-lanes").fill("four");
  await page.getByTestId("survey-save").click();
  await expect(page.getByTestId("survey-problems")).toContainText("a survey answer is not one the question takes", { timeout: 15_000 });
  await page.getByTestId("survey-lanes").fill("4");
  await page.getByTestId("survey-surface-concrete").click();
  await page.getByTestId("survey-amenity-rear_boarding_pad-yes").click();
  await page.getByTestId("survey-ref-sign").fill("S-1180");
  // a photo, added at once and named by its hash (D-162)
  await page.getByTestId("survey-attachments-file").setInputFiles({ name: "approach.png", mimeType: "image/png", buffer: PNG });
  await expect(page.getByTestId("survey-attachments").getByTestId("attachment-img")).toBeVisible({ timeout: 15_000 });
  await page.getByRole("textbox", { name: "Caption for approach.png" }).fill("approaching");
  // the summary lists what differs from the record
  const review = page.getByTestId("survey-changes");
  await expect(review).toContainText("Traffic lanes");
  await expect(review).toContainText("Rear door pad");
  await expect(page.getByTestId("survey-review")).toContainText("1 photo");
  await page.getByTestId("survey-save").click();
  await expect(page.getByTestId("status")).toContainText("Survey of Main & 1st on 2026-11-02 saved.", { timeout: 15_000 });
  // the surveyor is the person writing — with sign-in off, "anonymous"
  await expect(page.getByTestId("stop-surveyed")).toContainText("Last surveyed on 2026-11-02 by anonymous.");
  const site = page.getByTestId("stop-site");
  await expect(site.first()).toContainText("On street");
  await expect(site.first()).toContainText("Main St");
  await expect(site.first()).toContainText("Far side");
  await expect(page.getByTestId("stop-refs")).toContainText("S-1180");
  await expect(page.getByTestId("stop-amenities")).toContainText("Rear door pad");
  const newest = page.getByTestId("stop-event").first();
  await expect(newest).toContainText("Surveyed");
  await expect(newest).toContainText("by anonymous");
  await expect(newest.getByTestId("attachment-img")).toBeVisible();
  // saved: no draft is kept
  await expect(page.getByTestId("action-survey")).toHaveText("Survey");
});

test("on a phone a survey is the whole screen and never scrolls sideways", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=C");
  await page.getByTestId("action-survey").click();
  const box = await page.getByTestId("survey").boundingBox();
  expect(box.width).toBeGreaterThanOrEqual(388);
  expect(box.height).toBeGreaterThanOrEqual(840);
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  await expect(page.getByTestId("survey-save")).toBeInViewport();
  await expect(page.getByTestId("status")).toBeHidden();
  await page.getByTestId("survey-close").click();
  await expect(page.getByTestId("stop-card")).toBeVisible();
});

test("a stop request: made on its page, submitted, its work order opened, completed, and carried out", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=B");
  await expect(page.getByTestId("stop-requests")).toContainText("No requests.", { timeout: 15_000 });
  await page.getByTestId("action-request").click();
  await expect(page.getByTestId("request-submit")).toBeDisabled();
  await page.getByTestId("request-job-relocate_remove").click();
  await page.getByTestId("request-street").fill("Main St");
  await page.getByTestId("request-cross").fill("2nd Ave");
  await page.getByTestId("request-offset").fill("60");
  await page.getByTestId("request-side-south").click();
  await page.getByTestId("request-routes").fill("10");
  await page.getByTestId("request-attachments-file").setInputFiles({ name: "site drawing.pdf", mimeType: "application/pdf", buffer: Buffer.from("%PDF-1.4 a drawing") });
  await expect(page.getByTestId("request-attachments")).toContainText("site drawing.pdf", { timeout: 15_000 });
  await expect(page.getByTestId("request-id")).toHaveValue("B-20261102");
  await page.getByTestId("request-submit").click();
  const view = page.getByTestId("request-view");
  await expect(page.getByTestId("request-track-submitted")).toHaveClass(/now/, { timeout: 15_000 });
  await expect(view).toContainText("Relocate / remove");
  await expect(page.getByTestId("request-part-where")).toContainText("60");
  await expect(view.getByRole("link", { name: "site drawing.pdf" })).toBeVisible();

  // the utility locate, edited in place
  await page.getByTestId("request-edit-locate").click();
  await page.getByTestId("request-ticket").fill("26123456");
  await page.getByTestId("request-save-locate").click();
  await expect(page.getByTestId("request-part-locate")).toContainText("26123456", { timeout: 15_000 });

  // the stop's change names the request it carries out
  await page.getByTestId("request-back").click();
  await change(page, "details");
  await page.getByTestId("form-name").fill("Main & 2nd (relocated)");
  await page.getByTestId("form-request").selectOption("B-20261102");
  await page.getByTestId("form-save").click();
  // (history is by date: another test's later move on B lists above it)
  const carried = page.getByTestId("stop-event").filter({ hasText: "(request B-20261102)" });
  await expect(carried.filter({ hasText: "relocated" })).toHaveCount(1, { timeout: 15_000 });

  // its work order opened (the stub adapter), completed, then the change
  // it carried out recorded from it
  await page.getByTestId("request-row").click();
  await page.getByTestId("request-wo-open").click();
  await expect(page.getByTestId("request-complete")).toBeVisible({ timeout: 15_000 });
  await page.getByTestId("request-complete").click();
  await page.getByTestId("request-completed-on").fill("2026-11-20");
  await page.getByTestId("request-save").click();
  await expect(page.getByTestId("request-track-completed")).toHaveClass(/now/, { timeout: 15_000 });
  await page.getByTestId("request-record").click();
  await expect(page.getByTestId("change-choices")).toBeVisible();
  await expect(page.getByTestId("change")).toContainText("Request B-20261102");
  await page.getByTestId("choose-details").click();
  await page.getByTestId("form-amenity-bench-yes").click();
  await page.getByTestId("form-save").click();
  await expect(carried).toHaveCount(2, { timeout: 15_000 });
});

test("a field report, a request cancelled, and the work across every stop", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=A");
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 15_000 });
  // a field report: a draft repair request, its photo
  await page.getByTestId("action-report").click();
  await expect(page.getByTestId("request-job-repair_improve")).toHaveAttribute("aria-pressed", "true");
  await page.getByTestId("request-note").fill("Sign post leaning");
  await page.getByTestId("request-attachments-camera").setInputFiles({ name: "post.png", mimeType: "image/png", buffer: PNG });
  await expect(page.getByTestId("request-attachments").getByTestId("attachment-img")).toBeVisible({ timeout: 15_000 });
  await page.getByTestId("request-save-draft").click();
  await expect(page.getByTestId("status")).toContainText("Request A-20261102 saved as a draft.", { timeout: 15_000 });
  await expect(page.getByTestId("request-track-drafted")).toHaveClass(/now/);
  // a second report the same day gets an id of its own
  await page.getByTestId("request-back").click();
  await page.getByTestId("action-report").click();
  await page.getByTestId("request-note").fill("Bench missing");
  await page.getByTestId("request-save-draft").click();
  await expect(page.getByTestId("status")).toContainText("Request A-20261102-2 saved as a draft.", { timeout: 15_000 });

  // submitted, then cancelled after a confirmation
  await page.getByTestId("request-submit-now").click();
  await expect(page.getByTestId("request-track-submitted")).toHaveClass(/now/, { timeout: 15_000 });
  await page.getByTestId("request-cancel").click();
  await page.getByTestId("request-cancel-yes").click();
  await expect(page.getByTestId("request-status")).toHaveText("Cancelled", { timeout: 15_000 });

  // the work across every stop: open work by default, filtered by job
  await page.getByTestId("view-work").click();
  const queue = page.getByTestId("work-queue");
  await expect(queue).toContainText("Sign post leaning", { timeout: 15_000 });
  await expect(queue).not.toContainText("Bench missing");
  await expect(page.getByTestId("work-filter-cancelled")).toContainText("1");
  await page.getByTestId("work-filter-cancelled").click();
  await expect(queue).toContainText("Bench missing");
  await page.getByTestId("work-job-new").click();
  await expect(queue).not.toContainText("Sign post leaning");
  await page.getByTestId("work-job-all").click();
  await page.getByTestId("work-row").filter({ hasText: "Sign post leaning" }).click();
  await expect(page.getByTestId("request-view")).toContainText("Sign post leaning", { timeout: 15_000 });
  await expect(page).toHaveURL(/open=A/);
  // back to the queue it came from
  await page.getByTestId("request-back").click();
  await expect(queue).toBeVisible();
});

test("a request's work order opened and read through the work order system's adapter", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=C");
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 15_000 });
  await page.getByTestId("action-request").click();
  await page.getByTestId("request-job-repair_improve").click();
  await page.getByTestId("request-submit").click();
  await expect(page.getByTestId("request-track-submitted")).toHaveClass(/now/, { timeout: 15_000 });
  await page.getByTestId("request-wo-open").click();
  await expect(page.getByTestId("status")).toContainText("Work order STUB-C-20261102 opened for C-20261102.", { timeout: 15_000 });
  await expect(page.getByTestId("request-track-ordered")).toHaveClass(/now/);
  await expect(page.getByTestId("request-next-step")).toContainText("Work order STUB-C-20261102 in Maintenance: open (read 2026-08-01)");
  await page.getByTestId("request-wo-refresh").click();
  await expect(page.getByTestId("status")).toContainText("Work order STUB-C-20261102: open.", { timeout: 15_000 });
});

test("the stops near the device, nearest first", async ({ page, context }) => {
  await context.grantPermissions(["geolocation"]);
  await context.setGeolocation({ latitude: 45.5201, longitude: -73.5201, accuracy: 5 });
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02");
  await page.getByTestId("near-me").click();
  const rows = page.getByTestId("near-row");
  await expect(rows.first()).toContainText("Main & 3rd", { timeout: 15_000 });
  await expect(rows.first()).toContainText(/\d+ m/);
  await expect(page.locator('[data-testid="map-key-me"]:visible')).toBeVisible();
  await rows.first().click();
  await expect(page.getByTestId("stop-card")).toContainText("Main & 3rd");
  await page.getByTestId("stop-close").click();
  await page.getByTestId("near-clear").click();
  await expect(page.getByTestId("stop-list")).toBeVisible();
});

test("the stops' map is kept on the way to a stop and back, and the stops read as a table", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02");
  await expect(page.getByTestId("stop-row").first()).toBeVisible({ timeout: 15_000 });
  const before = await shownMap(page).locator("canvas").elementHandle();
  await page.getByTestId("stop-row").filter({ hasText: "Main & 3rd" }).click();
  await expect(page.getByTestId("stop-card")).toContainText("Main & 3rd");
  await page.getByTestId("stop-close").click();
  const after = await shownMap(page).locator("canvas").elementHandle();
  expect(await page.evaluate(([a, b]) => a === b, [before, after])).toBe(true);
  await page.getByTestId("mode-table").click();
  await expect(page.getByTestId("stops-table")).toContainText("Main & 3rd");
  await page.getByTestId("stops-table").getByTestId("stop-row").filter({ hasText: "Main & 3rd" }).click();
  await expect(page.getByTestId("stop-card")).toContainText("Main & 3rd");
});

test("what the agency's feed did to the stops record: reviewed, each stop linked", async ({ page, request }) => {
  // the record as the server's import leaves it (D-150): a change naming
  // the feed it came from, and a stop in service the feed does not list
  const manifest = await (await request.get("/manifest")).json();
  const doc = await (await request.get(`/blobs/${manifest.stops_hash}`)).json();
  doc.stops.A.push({ on: "2026-11-21", event: "changed", name: "Main & First", from_feed: manifest.feed_hash });
  doc.stops.ZZ = [{ event: "in_service", lat: 45.505, lon: -73.505, name: "Old Mill" }];
  const written = await request.post("/stops", { data: { from: manifest.stops_hash, stops: doc } });
  expect(written.status(), await written.text()).toBe(200);

  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02");
  await page.getByTestId("view-feed").click();
  const notice = page.getByTestId("feed-notice");
  await expect(notice).toContainText("recorded 1 change; does not list 1 stop in service", { timeout: 15_000 });
  await expect(notice).toContainText("A changed (name), from 2026-11-21");
  await notice.getByRole("button", { name: "ZZ" }).click();
  await expect(page.getByTestId("stop-card")).toContainText("Old Mill");
});

// A pick only reads the stops (D-157): published over stop C, retired
// before the pick's first date, the publish answers the warning, and the
// service-change page shows it with the plan's others. The stops record
// is the one this server keeps (begun from its network when none is).
test("a pick published over a retired stop carries the warning", async ({ page, request }) => {
  const manifest = await (await request.get("/manifest")).json();
  let doc;
  if (manifest.stops_hash) {
    doc = await (await request.get(`/blobs/${manifest.stops_hash}`)).json();
  } else {
    const { network } = await (await request.get("/network")).json();
    doc = { kind: "stops", schema_version: 1, stops: {} };
    for (const [id, s] of Object.entries(network.stops)) {
      doc.stops[id] = [{ event: "in_service", lat: s.lat, lon: s.lon, ...(s.name ? { name: s.name } : {}), ...(s.parent ? { parent: s.parent } : {}), ...(s.kind === "station" ? { kind: "station" } : {}) }];
    }
  }
  doc.stops.C.push({ on: "2026-09-01", event: "retired", note: "Replaced by the new stop at Main" });
  const written = await request.post("/stops", { data: { from: manifest.stops_hash ?? "", stops: doc } });
  expect(written.status(), await written.text()).toBe(200);

  const { network } = await (await request.get("/network")).json();
  const pick = { ...network, name: "Late 2026 pick", plan: { id: "late-2026", name: "Late 2026 pick", effective_from: "2026-12-14" } };
  const published = await request.post("/plans", { data: JSON.stringify(pick) });
  expect(published.status(), await published.text()).toBe(200);
  const { warnings } = await published.json();
  expect(warnings).toContainEqual({ stop_not_in_service: { stop: "C", status: "retired", from: "2026-12-14" } });

  await page.clock.setFixedTime(new Date("2026-12-16T09:00:00-05:00"));
  await page.goto("/service-change.html#stage=proposal");
  await expect(page.getByTestId("plan-select")).toHaveValue("published:late-2026", { timeout: 15_000 });
  await expect(page.getByTestId("stage-proposal")).toContainText("The plan stops at C, which is retired from 2026-12-14 on", { timeout: 15_000 });
});

test.describe("signed in", () => {
  test.use({ baseURL: "http://127.0.0.1:8090" });

  test("a write needs sign-in; signed in, the surveyor is the person", async ({ page }) => {
    await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
    await page.goto("/stops.html#on=2026-11-02&open=A");
    await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId("signin-button")).toBeVisible();

    // not signed in: refused in words beside Save, the survey's draft kept
    await page.getByTestId("action-survey").click();
    await page.getByTestId("survey-on_street").fill("Main St");
    await page.getByTestId("survey-save").click();
    await expect(page.getByTestId("survey-problems")).toContainText("Sign in to save", { timeout: 15_000 });
    await page.getByTestId("survey-close").click();
    await expect(page.getByTestId("action-survey")).toHaveText("Continue survey");

    // out to the provider and back to the same stop
    await page.getByTestId("signin-button").click();
    await expect(page.getByTestId("signin-name")).toHaveText("Rosa Ortiz", { timeout: 15_000 });
    await expect(page).toHaveURL(/stops\.html#.*open=A/);
    await expect(page).not.toHaveURL(/[?&]code=/);

    await page.getByTestId("action-survey").click();
    await expect(page.getByTestId("survey-on_street")).toHaveValue("Main St");
    await page.getByTestId("survey-attachments-camera").setInputFiles({ name: "sign.png", mimeType: "image/png", buffer: PNG });
    await expect(page.getByTestId("survey-review")).toContainText("1 photo", { timeout: 15_000 });
    await page.getByTestId("survey-save").click();
    await expect(page.getByTestId("stop-surveyed")).toContainText("Last surveyed on 2026-11-02 by Rosa Ortiz.", { timeout: 15_000 });

    await page.getByTestId("signout").click();
    await expect(page.getByTestId("signin-button")).toBeVisible();
  });
});
