// The stops UI (src/uis/stops, D-157): every stop's own life, apart from
// the service plans. Its own server (playwright.config.mjs, :8084) runs
// the durable store, which a stops write needs (POST /stops); no stops
// document is kept until the first change, which saves the stops of the
// network running today (cap-stops `seed`).
//
//   - a stop in service is read on a date, with the routes that stop
//     there in the schedule running that day;
//   - suspended for dates (a refused end before its start first), it
//     reads suspended on a day inside them, counted and listed so;
//   - it returns early, the return's date corrected, and it moves —
//     the map's click fills the point's fields, and a measured point
//     typed over it is the one saved, to the digit;
//   - a new stop is planned at a point typed, permitted, and
//     retired — hidden from the map until retired stops are shown;
//   - a stop is surveyed (D-158): the half-filled survey survives a
//     reload on this device, a count that is not a number is stopped at
//     its field, the surveyor is the person writing (the server's, never
//     typed), a photo is added and named by its hash, and the saved
//     survey shows on the card grouped as the
//     worksheet groups it, with its surveyor and date;
//   - a request for work at a stop (D-159) is started, refused in words
//     while it claims an open work order it does not name, saved with
//     the work order, and named by the stop change that carries it out;
//   - the work order system through its adapter (D-165; the stub on this
//     server): a submitted request's work order opened, then its status
//     read — the answers written on the request;
//   - signed in (D-58, D-163; its own server on :8090 with sign-in on,
//     and a stand-in provider on :8089): a write before signing in is
//     refused in words and its draft kept; signing in goes out to the
//     provider and back to the same stop; the survey then saves, its
//     surveyor the signed-in person's name, and a photo added and shown
//     only to the signed-in;
//   - what the agency's feed did to the stops record (D-150: the feed
//     applies, and the planner sees what it did): one line, reviewed,
//     each stop linked to its card.

import { test, expect } from "@playwright/test";

/// A 1x1 PNG: a photo as small as a test needs.
const PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==", "base64");

test.use({ baseURL: "http://127.0.0.1:8084" });

test("a stop's life: suspended and back, moved, and a new stop planned, permitted, retired", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=B");
  const card = page.getByTestId("stop-card");
  await expect(card).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("stop-status")).toContainText("In service");
  await expect(page.getByTestId("stop-routes")).not.toContainText("No route");

  // A suspension whose end comes before its start is refused, in words.
  await page.getByTestId("action-suspended").click();
  await page.getByTestId("form-on").fill("2026-11-09");
  await page.getByTestId("form-until").fill("2026-11-05");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-refused")).toContainText("the end must come after the first day");

  // Suspended Monday 11-09 until Thursday 11-12, for paving.
  await page.getByTestId("action-suspended").click();
  await page.getByTestId("form-on").fill("2026-11-09");
  await page.getByTestId("form-until").fill("2026-11-12");
  await page.getByTestId("form-note").fill("Paving");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("status")).toContainText("Suspend: B from 2026-11-09 until 2026-11-12", { timeout: 15_000 });

  await page.getByTestId("stops-on").fill("2026-11-10");
  await expect(page.getByTestId("stop-status")).toContainText("Suspended since 2026-11-09, back 2026-11-12");
  await expect(page.getByTestId("stop-status")).toContainText("Paving");
  await expect(page.getByTestId("filter-suspended")).toContainText("1");
  await expect(page.getByTestId("map-key-suspended")).toBeVisible();

  // Back early on Wednesday — entered as Thursday, then corrected.
  await page.getByTestId("action-in_service").click();
  await page.getByTestId("form-on").fill("2026-11-10");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-status")).toContainText("In service since 2026-11-10", { timeout: 15_000 });
  const events = page.getByTestId("stop-event");
  const last = (await events.count()) - 1;
  await page.getByTestId(`fix-${last}`).click();
  await page.getByTestId("fix-on").fill("2026-11-11");
  await page.getByTestId("fix-save").click();
  await expect(page.getByTestId("status")).toContainText("Corrected in service on B", { timeout: 15_000 });
  await expect(events.last()).toContainText("2026-11-11");
  await expect(page.getByTestId("stop-status")).toContainText("Suspended", { timeout: 10_000 });

  // Moved: a click on the map fills the point's fields; the measured
  // point typed over it is the one saved, to the digit.
  await page.getByTestId("stops-on").fill("2026-11-20");
  await expect(page.getByTestId("stop-status")).toContainText("In service");
  await page.getByTestId("action-move").click();
  await expect(page.getByTestId("form-save")).toBeDisabled();
  await page.getByTestId("stop-life-map").locator("canvas").click({ position: { x: 300, y: 300 } });
  await expect(page.getByTestId("move-point-lat")).toHaveValue(/^-?\d+(\.\d+)?$/);
  await expect(page.getByTestId("move-point-lon")).toHaveValue(/^-?\d+(\.\d+)?$/);
  await page.getByTestId("move-point-lat").fill("45.5123456");
  await page.getByTestId("move-point-lon").fill("-73.5123456");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("status")).toContainText("Moved B from 2026-11-20", { timeout: 15_000 });
  await expect(events.last()).toContainText("Moved to 45.5123456, -73.5123456");
  await expect(page.getByTestId("stop-location")).toHaveText("45.5123456, -73.5123456");

  // A new stop, planned at a point typed (no map click), then permitted.
  await page.getByTestId("new-stop-start").click();
  await expect(page.getByTestId("new-save")).toBeDisabled();
  await page.getByTestId("new-point-lat").fill("45.5051234");
  await page.getByTestId("new-point-lon").fill("-73.5051234");
  await page.getByTestId("new-id").fill("N1");
  await page.getByTestId("new-name").fill("Main & 9th");
  await page.getByTestId("new-save").click();
  await expect(page.getByTestId("status")).toContainText("Added stop N1", { timeout: 15_000 });
  await expect(page.getByTestId("stop-status")).toContainText("Planned since 2026-11-20");
  await expect(page.getByTestId("stop-location")).toHaveText("45.5051234, -73.5051234");
  await expect(page.getByTestId("action-in_service")).toBeVisible();
  await expect(page.getByTestId("action-suspended")).toHaveCount(0);
  await page.getByTestId("action-permitted").click();
  await page.getByTestId("form-on").fill("2026-11-20");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-status")).toContainText("Permitted", { timeout: 15_000 });

  // Retired: the stop leaves the map and the list until retired stops
  // are shown.
  await page.getByTestId("action-retired").click();
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-status")).toContainText("Retired", { timeout: 15_000 });
  await expect(page.getByTestId("filter-retired")).toContainText("1");
  await page.getByRole("button", { name: "Close" }).click();
  const rows = page.getByTestId("stop-row");
  await expect(rows.filter({ hasText: "Main & 9th" })).toHaveCount(0);
  await page.getByTestId("filter-retired").click();
  await expect(rows.filter({ hasText: "Main & 9th" })).toHaveCount(1);
});

test("a stop's site survey: kept as a draft, checked, saved, and shown", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=A");
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("stop-surveyed")).toHaveText("Never surveyed.");
  // what the schedule says beside the survey: the stop beside it, and how far
  await expect(page.getByTestId("stop-visits")).toContainText(/(before|after) Main & 2nd \((about )?\d+ m\)/);

  // Half filled, then the tab is reloaded: the survey is back.
  await page.getByTestId("action-survey").click();
  await page.getByTestId("survey-position").selectOption("far_side");
  await page.getByTestId("survey-on_street").fill("Main St");
  await page.reload();
  await expect(page.getByTestId("action-survey")).toHaveText("Survey (draft)", { timeout: 15_000 });
  await page.getByTestId("action-survey").click();
  await expect(page.getByTestId("survey-restored")).toBeVisible();
  await expect(page.getByTestId("survey-on_street")).toHaveValue("Main St");
  await expect(page.getByTestId("survey-position")).toHaveValue("far_side");

  // A count that is not a whole number is stopped at the field (the
  // write would refuse it too: cap-stops' dispatch test), and nothing
  // is lost.
  await page.getByTestId("survey-lanes").fill("four");
  await page.getByTestId("survey-save").click();
  await expect(page.getByTestId("survey-form")).toBeVisible();
  expect(await page.getByTestId("survey-lanes").evaluate((el) => el.validity.valid)).toBe(false);
  await page.getByTestId("survey-lanes").fill("4");
  await page.getByTestId("survey-surface").selectOption("concrete");
  await page.getByTestId("survey-amenity-rear_boarding_pad").selectOption("yes");
  await page.getByTestId("survey-ref-sign").fill("S-1180");
  // a photo, added at once and named by its hash (D-162)
  await page.getByTestId("survey-attachments-file").setInputFiles({ name: "approach.png", mimeType: "image/png", buffer: PNG });
  await expect(page.getByTestId("survey-attachments").getByTestId("attachment-img")).toBeVisible({ timeout: 15_000 });
  await page.getByRole("textbox", { name: "Caption for approach.png" }).fill("approaching");
  await page.getByTestId("survey-save").click();
  await expect(page.getByTestId("status")).toContainText("Survey of A on 2026-11-02 saved.", { timeout: 15_000 });
  // the surveyor is the person writing — with sign-in off, "anonymous"
  await expect(page.getByTestId("stop-surveyed")).toHaveText("Last surveyed on 2026-11-02 by anonymous.");
  const site = page.getByTestId("stop-site");
  await expect(site.first()).toContainText("On street");
  await expect(site.first()).toContainText("Main St");
  await expect(site.first()).toContainText("Far side");
  await expect(page.getByTestId("stop-refs")).toContainText("S-1180");
  await expect(page.getByTestId("stop-amenities")).toContainText("Rear door pad: there");
  await expect(page.getByTestId("stop-event").last()).toContainText("Surveyed by anonymous");
  await expect(page.getByTestId("stop-event").last().getByTestId("attachment-img")).toBeVisible();
  await expect(page.getByTestId("stop-event").last()).toContainText("approaching");
  // saved: no draft is kept
  await expect(page.getByTestId("action-survey")).toHaveText("Survey");
});

test("a stop request: started, checked, tracking its work order, and carried out", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=B");
  await expect(page.getByTestId("stop-requests")).toContainText("No requests for this stop.", { timeout: 15_000 });
  await page.getByTestId("request-new").click();
  await expect(page.getByTestId("request-id")).toHaveValue("B-20261102");
  await page.getByTestId("request-job").selectOption("relocate_remove");
  await page.getByTestId("request-routes").fill("10");
  await page.getByTestId("request-street").fill("Main St");
  await page.getByTestId("request-cross").fill("2nd Ave");
  await page.getByTestId("request-offset").fill("60");
  await page.getByTestId("request-side").selectOption("south");
  await page.getByTestId("request-ticket").fill("26123456");
  await page.getByTestId("request-attachments-file").setInputFiles({ name: "site drawing.pdf", mimeType: "application/pdf", buffer: Buffer.from("%PDF-1.4 a drawing") });
  await expect(page.getByTestId("request-attachments")).toContainText("site drawing.pdf", { timeout: 15_000 });
  await page.getByTestId("request-status").selectOption("ordered");
  await page.getByTestId("request-save").click();
  await expect(page.getByTestId("request-refused")).toContainText("needs the work order's id", { timeout: 15_000 });
  await page.getByTestId("request-wo-system").fill("Maintenance");
  await page.getByTestId("request-wo-id").fill("WO-88123");
  await page.getByTestId("request-wo-status").fill("open");
  await page.getByTestId("request-save").click();
  await expect(page.getByTestId("status")).toContainText("Request B-20261102 saved.", { timeout: 15_000 });
  const row = page.getByTestId("request-row");
  await expect(row).toHaveCount(1);
  await expect(row).toContainText("Relocate/Remove");
  await expect(row).toContainText("Work order open");
  await expect(row).toContainText("Maintenance WO-88123 · open");
  await expect(row.getByRole("link", { name: "site drawing.pdf" })).toBeVisible();

  // the stop's change names the request it carries out
  await page.getByTestId("action-details").click();
  await page.getByTestId("form-name").fill("Main & 2nd (relocated)");
  await page.getByTestId("form-request").selectOption("B-20261102");
  await page.getByTestId("form-save").click();
  await expect(page.getByTestId("stop-event").filter({ hasText: "relocated" })).toContainText("(request B-20261102)", { timeout: 15_000 });

  // edited later: completed, the id kept
  await page.getByTestId("request-edit").click();
  await expect(page.getByTestId("request-id")).toHaveValue("B-20261102");
  await page.getByTestId("request-status").selectOption("completed");
  await page.getByTestId("request-completed-on").fill("2026-11-20");
  await page.getByTestId("request-save").click();
  await expect(row).toContainText("Completed", { timeout: 15_000 });
  await expect(row).toHaveCount(1);
});

test("a request's work order opened and read through the work order system's adapter", async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto("/stops.html#on=2026-11-02&open=C");
  await expect(page.getByTestId("stop-requests")).toBeVisible({ timeout: 15_000 });
  await page.getByTestId("request-new").click();
  await page.getByTestId("request-job").selectOption("repair_improve");
  await page.getByTestId("request-status").selectOption("submitted");
  await page.getByTestId("request-save").click();
  const row = page.getByTestId("request-row").filter({ hasText: "C-20261102" });
  await expect(row).toContainText("Submitted", { timeout: 15_000 });
  await row.getByTestId("request-wo-open").click();
  await expect(page.getByTestId("status")).toContainText("Work order STUB-C-20261102 opened for C-20261102.", { timeout: 15_000 });
  await expect(row).toContainText("Work order open");
  await expect(row).toContainText("Maintenance STUB-C-20261102 · open (2026-08-01)");
  await row.getByTestId("request-wo-refresh").click();
  await expect(page.getByTestId("status")).toContainText("Work order STUB-C-20261102: open.", { timeout: 15_000 });
});

test("what the agency's feed did to the stops record: one line, reviewed, each stop linked", async ({ page, request }) => {
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
  const notice = page.getByTestId("feed-notice");
  await expect(notice).toContainText("recorded 1 change; does not list 1 stop in service", { timeout: 15_000 });
  await page.getByTestId("feed-review").click();
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

    // not signed in: refused in words, the survey's draft kept
    await page.getByTestId("action-survey").click();
    await page.getByTestId("survey-on_street").fill("Main St");
    await page.getByTestId("survey-save").click();
    await expect(page.getByTestId("status")).toContainText("Sign in to save changes", { timeout: 15_000 });
    await expect(page.getByTestId("action-survey")).toHaveText("Survey (draft)");

    // out to the provider and back to the same stop
    await page.getByTestId("signin-button").click();
    await expect(page.getByTestId("signin-name")).toHaveText("Rosa Ortiz", { timeout: 15_000 });
    await expect(page).toHaveURL(/stops\.html#.*open=A/);
    await expect(page).not.toHaveURL(/[?&]code=/);

    await page.getByTestId("action-survey").click();
    await expect(page.getByTestId("survey-on_street")).toHaveValue("Main St");
    await page.getByTestId("survey-attachments-camera").setInputFiles({ name: "sign.png", mimeType: "image/png", buffer: PNG });
    await expect(page.getByTestId("survey-attachments").getByTestId("attachment-img")).toBeVisible({ timeout: 15_000 });
    await page.getByTestId("survey-save").click();
    await expect(page.getByTestId("stop-surveyed")).toHaveText("Last surveyed on 2026-11-02 by Rosa Ortiz.", { timeout: 15_000 });

    await page.getByTestId("signout").click();
    await expect(page.getByTestId("signin-button")).toBeVisible();
  });
});
