// The stops UI (src/uis/stops: surveys, requests, feed review, offline, and
// warnings; D-233). The server
// on :8084 runs the durable store, which a stops write needs (POST /stops). No
// stops document exists until the first change seeds it from the network
// running today (cap-stops `seed`). The sign-in tests use a server on :8090
// with sign-in on and a stand-in provider on :8089.

import { test, expect } from "@playwright/test";

const PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==", "base64");

test.use({ baseURL: "http://127.0.0.1:8084" });

async function change(page, kind) {
  await page.getByTestId("action-change").click();
  await page.getByTestId(`action-${kind}`).click();
  await expect(page.getByTestId("change")).toBeVisible();
}

/// The stops' map is kept, hidden, under other screens.
const shownMap = (page) => page.locator('[data-testid="stop-life-map"]:visible');
/// A click before the map loads places nothing.
const readyMap = (page) => page.locator('[data-testid="stop-life-map"][data-ready="true"]:visible');

test("a stop's life: suspended and back, moved, and a new stop planned, requested, permitted, retired", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=B");
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("stop-status")).toContainText("In service");
  await expect(page.getByTestId("stop-routes")).not.toContainText("No routes");

  await change(page, "suspended");
  await page.getByTestId("form-on").fill("2026-11-09");
  await page.getByTestId("form-temporary").click();
  await page.getByTestId("form-until").fill("2026-11-05");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("change-problems")).toContainText("the end must come after the first day", { timeout: 15_000 });

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

  await change(page, "in_service");
  await page.getByTestId("form-on").fill("2026-11-10");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-status")).toContainText("since 2026-11-10", { timeout: 15_000 });
  const events = page.getByTestId("stop-event");
  await expect(events.first()).toContainText("In service");
  await events.first().getByRole("button", { name: "Correct" }).click();
  await page.getByTestId("form-on").fill("2026-11-11");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("status")).toContainText("Corrected the in service event on Main & 2nd", { timeout: 15_000 });
  await expect(events.first()).toContainText("2026-11-11");
  await expect(page.getByTestId("stop-status")).toContainText("Suspended", { timeout: 10_000 });
  await expect(page.getByTestId("stop-status")).toContainText("until 2026-11-11");

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
  await change(page, "details");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("form-unchanged")).toBeVisible();
  await expect(page.getByTestId("change")).toBeVisible();
  await page.goBack();
  await expect(page.getByTestId("change")).toHaveCount(0);
  await expect(page.getByTestId("stop-card")).toBeVisible();

  await page.getByTestId("new-stop-start").click();
  await expect(page.getByTestId("new-next")).toBeDisabled();
  await page.getByTestId("new-point-lat").fill("45.5051234");
  await page.getByTestId("new-point-lon").fill("-73.5051234");
  await page.getByTestId("new-next").click();
  await page.getByTestId("new-id").fill("B");
  await page.getByTestId("new-name").fill("Main & 9th");
  await page.getByTestId("new-save").click();
  await expect(page.getByTestId("new-stop-problems")).toContainText("there is already a stop with that id");
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

  await change(page, "retired");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-status")).toContainText("Retired", { timeout: 15_000 });
  await page.getByTestId("stop-close").click();
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
  await expect(page.getByTestId("stop-visits")).toContainText(/(before|after) Main & 2nd \(~?\d+ m\)/);
  await expect(page.getByTestId("stop-surveyed")).toContainText("Never surveyed.");

  await page.getByTestId("action-survey").click();
  await expect(page.getByTestId("survey-nav-s-location")).toBeVisible();
  await page.getByTestId("survey-on_street").fill("Main St");
  await page.getByTestId("survey-position-far_side").click();
  await page.reload();
  await expect(page.getByTestId("survey-restored")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("survey-on_street")).toHaveValue("Main St");
  await expect(page.getByTestId("survey-position-far_side")).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("survey-nav-s-location")).toContainText("2/");
  await page.getByTestId("survey-close").click();
  await expect(page.getByTestId("action-survey")).toHaveText("Continue survey");
  await page.getByTestId("action-survey").click();

  await page.getByTestId("survey-nav-s-street").click();
  await page.getByTestId("survey-lanes").fill("four");
  await page.getByTestId("survey-save").click();
  await expect(page.getByTestId("survey-problems")).toContainText("a survey answer is not one the question takes", { timeout: 15_000 });
  await page.getByTestId("survey-lanes").fill("4");
  await page.getByTestId("survey-surface-concrete").click();
  await page.getByTestId("survey-amenity-rear_boarding_pad-yes").click();
  await page.getByTestId("survey-ref-sign").fill("S-1180");
  await page.getByTestId("survey-attachments-file").setInputFiles({ name: "approach.png", mimeType: "image/png", buffer: PNG });
  await expect(page.getByTestId("survey-attachments").getByTestId("attachment-img")).toBeVisible({ timeout: 15_000 });
  await page.getByRole("textbox", { name: "Caption for approach.png" }).fill("approaching");
  const review = page.getByTestId("survey-changes");
  await expect(review).toContainText("Traffic lanes");
  await expect(review).toContainText("Rear door pad");
  await expect(page.getByTestId("survey-review")).toContainText("1 photo");
  await page.getByTestId("survey-save").click();
  await expect(page.getByTestId("status")).toContainText("Survey of Main & 1st on 2026-11-02 saved.", { timeout: 15_000 });
  // With sign-in off, the surveyor is "anonymous".
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

test("a stop whose routes cannot be read says so, never that no route stops there", async ({ page }) => {
  await page.route(/\/caps\/(ui-stops|directory)-[0-9a-f]+\.wasm$/, (route) => route.abort());
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=A");
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByTestId("stop-routes")).toContainText("The routes are not available: This part of the app did not load.");
});

// A failed read of the stops document is never shown as a deployment with no
// stops record, and the page never seeds the schedule's stops over it.
test("a stops document whose read fails says so, never that the deployment publishes no stops yet", async ({ page }) => {
  const pin = "f".repeat(64);
  await page.route("**/manifest", async (route) => {
    const resp = await route.fetch();
    const manifest = await resp.json();
    await route.fulfill({ response: resp, json: { ...manifest, stops_hash: pin } });
  });
  await page.route(`**/blobs/${pin}.gz`, (route) => route.fulfill({ status: 503, body: "the store is not answering" }));
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html");
  await expect(page.getByTestId("status")).toContainText("Stops could not be read", { timeout: 15_000 });
  await expect(page.getByText("publishes no stops yet")).toHaveCount(0);
  await expect(page.getByText("stops from the schedule running today")).toHaveCount(0);
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

  await page.getByTestId("request-edit-locate").click();
  await page.getByTestId("request-ticket").fill("26123456");
  await page.getByTestId("request-save-locate").click();
  await expect(page.getByTestId("request-part-locate")).toContainText("26123456", { timeout: 15_000 });

  await page.getByTestId("request-back").click();
  await change(page, "details");
  await page.getByTestId("form-name").fill("Main & 2nd (relocated)");
  await page.getByTestId("form-request").selectOption("B-20261102");
  await page.getByTestId("form-save").click();
  // History is by date, so another test's later move on B lists above it.
  const carried = page.getByTestId("stop-event").filter({ hasText: "(request B-20261102)" });
  await expect(carried.filter({ hasText: "relocated" })).toHaveCount(1, { timeout: 15_000 });

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
  await page.getByTestId("action-report").click();
  await expect(page.getByTestId("request-job-repair_improve")).toHaveAttribute("aria-pressed", "true");
  await page.getByTestId("request-note").fill("Sign post leaning");
  await page.getByTestId("request-attachments-camera").setInputFiles({ name: "post.png", mimeType: "image/png", buffer: PNG });
  await expect(page.getByTestId("request-attachments").getByTestId("attachment-img")).toBeVisible({ timeout: 15_000 });
  await page.getByTestId("request-save-draft").click();
  await expect(page.getByTestId("status")).toContainText("Request A-20261102 saved as a draft.", { timeout: 15_000 });
  await expect(page.getByTestId("request-track-drafted")).toHaveClass(/now/);
  await page.getByTestId("request-back").click();
  await page.getByTestId("action-report").click();
  await page.getByTestId("request-note").fill("Bench missing");
  await page.getByTestId("request-save-draft").click();
  await expect(page.getByTestId("status")).toContainText("Request A-20261102-2 saved as a draft.", { timeout: 15_000 });

  await page.getByTestId("request-submit-now").click();
  await expect(page.getByTestId("request-track-submitted")).toHaveClass(/now/, { timeout: 15_000 });
  await page.getByTestId("request-cancel").click();
  await page.getByTestId("request-cancel-yes").click();
  await expect(page.getByTestId("request-status")).toHaveText("Cancelled", { timeout: 15_000 });

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
  // A change that names its feed, and a stop that the feed does not list.
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

// A pick only reads the stops. The stops record is the one this server
// keeps, begun from its network when none exists.
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

    await page.getByTestId("action-survey").click();
    await page.getByTestId("survey-on_street").fill("Main St");
    await page.getByTestId("survey-save").click();
    await expect(page.getByTestId("status")).toContainText("Saved on this device. Sign in to send it.", { timeout: 15_000 });
    await expect(page.getByTestId("sync-waiting")).toContainText("Sign in to send 1 change");

    await page.getByTestId("signin-button").click();
    await expect(page.getByTestId("signin-name")).toHaveText("Rosa Ortiz", { timeout: 15_000 });
    await expect(page).toHaveURL(/stops\.html#.*open=A/);
    await expect(page).not.toHaveURL(/[?&]code=/);
    await expect(page.getByTestId("sync-status")).toHaveCount(0, { timeout: 30_000 });
    await expect(page.getByTestId("stop-surveyed")).toContainText("Last surveyed on 2026-11-02 by Rosa Ortiz.", { timeout: 15_000 });

    await page.getByTestId("action-survey").click();
    await page.getByTestId("survey-on_street").fill("Main Street");
    await page.getByTestId("survey-attachments-camera").setInputFiles({ name: "sign.png", mimeType: "image/png", buffer: PNG });
    await expect(page.getByTestId("survey-review")).toContainText("1 photo", { timeout: 15_000 });
    await page.getByTestId("survey-save").click();
    await expect(page.getByTestId("stop-surveyed")).toContainText("Last surveyed on 2026-11-02 by Rosa Ortiz.", { timeout: 15_000 });
    await expect(page.getByTestId("sync-status")).toHaveCount(0);

    await page.getByTestId("signout").click();
    await expect(page.getByTestId("signin-button")).toBeVisible();
  });
});

// D-233. server/requests_test.go pins the server's answer. This test pins the
// page's side, over the real answer with a broken reference added.
test("a write kept with a broken reference says so until dismissed", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.route("**/logs/stops", async (route) => {
    const resp = await route.fetch();
    const body = await resp.json();
    body.references = [{ log: "stops", kind: "stop_request", id: "STN-20261102", field: "/stop", target: "stop", names: "STN", entry: { log: "stops", seq: body.seq } }];
    await route.fulfill({ response: resp, json: body });
  });
  await page.goto("/stops.html#on=2026-11-02&open=STN");
  await page.getByTestId("action-request").click();
  await page.getByTestId("request-job-repair_improve").click();
  await page.getByTestId("request-save-draft").click();
  const warned = page.getByTestId("sync-warned");
  await expect(warned).toContainText("Saved with a warning", { timeout: 15_000 });
  await expect(warned).toContainText("Stop request STN-20261102 names stop STN, which isn't on record.");
  await page.getByTestId("sync-dismiss").click();
  await expect(warned).toHaveCount(0);
});

test("a request on a stop retired after the page read it is saved with the retirement as its warning", async ({ page, request }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  const post = async (key, op) => {
    const resp = await request.post("/logs/stops", { data: { key, ops: [op] } });
    expect(resp.status(), await resp.text()).toBe(200);
  };
  await post("e2e/rule-1", { cap: "stops", op: "add_event", args: { stop_id: "RULE", new: true,
    event: { on: "2026-01-01", event: "in_service", lat: 45.64, lon: -122.6, name: "Rule test stop" } } });
  await page.goto("/stops.html#on=2026-11-02&open=RULE");
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 15_000 });
  // The server's date is 2026-08-01.
  await post("e2e/rule-2", { cap: "stops", op: "add_event", args: { stop_id: "RULE", event: { on: "2026-07-01", event: "retired" } } });
  await page.getByTestId("action-request").click();
  await page.getByTestId("request-job-repair_improve").click();
  await page.getByTestId("request-save-draft").click();
  const warned = page.getByTestId("sync-warned");
  await expect(warned).toContainText("Saved with a warning", { timeout: 15_000 });
  await expect(warned).toContainText("names stop RULE, which was retired on 2026-07-01.");
});

test("an open request on a retired stop says so when its page opens", async ({ page, request }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  const post = async (key, op) => {
    const resp = await request.post("/logs/stops", { data: { key, ops: [op] } });
    expect(resp.status(), await resp.text()).toBe(200);
  };
  await post("e2e/open-1", { cap: "stops", op: "add_event", args: { stop_id: "RULE2", new: true,
    event: { on: "2026-01-01", event: "in_service", lat: 45.641, lon: -122.601, name: "Rule test stop 2" } } });
  await post("e2e/open-2", { cap: "requests", op: "put", args: { id: "R-RULE2", new: true,
    request: { stop: "RULE2", job: "repair_improve", status: "submitted", requested_on: "2026-06-01" } } });
  // The server's date is 2026-08-01.
  await post("e2e/open-3", { cap: "stops", op: "add_event", args: { stop_id: "RULE2", event: { on: "2026-07-01", event: "retired" } } });
  await page.goto("/stops.html#on=2026-11-02&open=RULE2&request=R-RULE2");
  await expect(page.getByTestId("request-warning")).toHaveText("This request names stop RULE2, which was retired on 2026-07-01.", { timeout: 15_000 });
});
