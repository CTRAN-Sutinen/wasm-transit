// The staff pages offline and their outbox (D-189; docs/architecture/updates-offline.md,
// "Offline"; web/src/outbox.ts). The spec takes the browser's network away with
// context.setOffline while the server keeps running. The server on :8095 is a
// store-mode server over a copy of the build.

import { test, expect } from "@playwright/test";

test.use({ baseURL: "http://127.0.0.1:8095", serviceWorkers: "allow", timezoneId: "UTC" });

async function controlled(page, url) {
  await page.goto(url);
  await expect.poll(() => page.evaluate(() => !!navigator.serviceWorker?.controller), { timeout: 20_000 }).toBe(true);
  await page.reload();
}

async function entries(request, log) {
  const answer = await request.get(`/logs/${log}`).then((r) => r.json());
  return answer.entries.map((e) => e.entry);
}

const cancel = {
  id: "c-offline",
  rev: 1,
  status: "accepted",
  updated_at: 1,
  cancel: { id: "cancel-t30", trip_id: "t30", start_secs: 31200, date: "2026-10-23" },
};

test("the disruptions page through an outage: a note and an alert wait, a reload keeps them, and each is sent once", async ({ page, context, request }) => {
  test.setTimeout(90_000);
  const posted = await request.post("/edits", { data: { kind: "edit_records", schema_version: 2, records: [cancel] } });
  expect(posted.ok()).toBeTruthy();
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await controlled(page, "/disruptions.html");
  const card = page.locator('[data-testid="route-card"][data-route="10"]');
  await expect(card.getByTestId("disruption-line").first()).toBeVisible({ timeout: 20_000 });

  await context.setOffline(true);

  await card.getByTestId("disruption-line").first().click();
  const detail = page.getByTestId("disruption-detail");
  await detail.getByTestId("note-box").getByLabel("Note").fill("Bus 12 swapped in.");
  await detail.getByTestId("add-note").click();
  await expect(page.getByTestId("sync-waiting")).toContainText("1 change waiting to send", { timeout: 15_000 });
  await expect(page.getByTestId("sync-offline")).toBeVisible();
  await detail.getByRole("button", { name: "‹ Back" }).click();

  await page.getByRole("navigation", { name: "Views" }).getByRole("button", { name: "Alerts" }).click();
  const gaps = page.getByTestId("gaps");
  await gaps.getByTestId("start-alert").first().click();
  const editor = page.getByTestId("alert-editor");
  await editor.getByTestId("header").fill("Route 30: the 8:40 AM trip is cancelled");
  await editor.getByTestId("publish").click();
  await expect(page.getByTestId("sync-waiting")).toContainText("2 changes waiting to send", { timeout: 15_000 });

  // The view is in the URL, so the reload opens Alerts again.
  await page.reload();
  await expect(page.getByTestId("gaps")).toContainText("8:40 AM trip cancelled", { timeout: 20_000 });
  await expect(page.getByTestId("sync-waiting")).toContainText("2 changes waiting to send");
  await expect(page.getByTestId("sync-offline")).toContainText("data as of");
  expect(await entries(request, "notes")).toHaveLength(0);

  // The page sends on its own when the device is online. The click is for a
  // person who does not wait.
  await context.setOffline(false);
  await page.getByTestId("sync-send").click({ timeout: 2_000 }).catch(() => {});
  await expect(page.getByTestId("sync-status")).toHaveCount(0, { timeout: 30_000 });
  const notes = await entries(request, "notes");
  const alerts = await entries(request, "alerts");
  expect(notes).toHaveLength(1);
  expect(alerts).toHaveLength(1);
  expect(notes[0].intent.claimed_at_ms).toBe(Date.parse("2026-10-23T07:55:00-04:00"));
  expect(notes[0].at_ms).not.toBe(notes[0].intent.claimed_at_ms);
  expect(notes[0].intent.key).toMatch(/^disruptions\//);
  await page.getByRole("navigation", { name: "Views" }).getByRole("button", { name: "Status" }).click();
  await page.locator('[data-testid="route-card"][data-route="10"]').getByTestId("disruption-line").first().click();
  await expect(page.getByTestId("detail-notes")).toContainText("Bus 12 swapped in.", { timeout: 15_000 });
});

async function change(page, kind) {
  await page.getByTestId("action-change").click();
  await page.getByTestId(`action-${kind}`).click();
  await expect(page.getByTestId("change")).toBeVisible();
}

async function suspend(page, from, until, note) {
  await change(page, "suspended");
  await page.getByTestId("form-on").fill(from);
  await page.getByTestId("form-temporary").click();
  await page.getByTestId("form-until").fill(until);
  await page.getByTestId("form-note").fill(note);
  await page.getByTestId("form-save").click();
}

async function otherWriter(request, key, stop, note) {
  const resp = await request.post("/logs/stops", {
    data: { key, ops: [{ cap: "stops", op: "add_event", args: { stop_id: stop, event: { on: "2026-11-03", event: "changed", note } } }] },
  });
  expect(resp.ok(), await resp.text()).toBeTruthy();
}

test("the stops page: a queued change runs over a moved head, and one refused at send time is kept with its reasons", async ({ page, context, request }) => {
  test.setTimeout(90_000);
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await controlled(page, "/stops.html#on=2026-11-02&open=B");
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 20_000 });
  await suspend(page, "2026-11-09", "2026-11-12", "Paving");
  await expect(page.getByTestId("status")).toContainText("Suspended", { timeout: 15_000 });
  await page.reload();
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 20_000 });

  await context.setOffline(true);
  await page.goto("/stops.html#on=2026-11-02&open=A");
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 20_000 });
  await suspend(page, "2026-11-16", "2026-11-18", "Sidewalk work");
  await expect(page.getByTestId("sync-waiting")).toContainText("1 change waiting to send", { timeout: 15_000 });
  await otherWriter(request, "desk/1", "B", "Bench replaced");

  await page.goto("/stops.html#on=2026-11-02&open=C");
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 20_000 });
  await suspend(page, "2026-11-20", "2026-11-21", "Tree trimming");
  await expect(page.getByTestId("sync-waiting")).toContainText("2 changes waiting to send", { timeout: 15_000 });
  await otherWriter(request, "desk/2", "C", "Shelter glass replaced");

  await context.setOffline(false);
  await page.getByTestId("sync-send").click({ timeout: 2_000 }).catch(() => {});
  const held = page.getByTestId("sync-held");
  await expect(held).toHaveCount(1, { timeout: 30_000 });
  await expect(held).toContainText("stop C");
  await expect(held).toContainText("Someone else changed this");
  await expect(page.getByTestId("sync-waiting")).toHaveCount(0);

  const stops = await request.get("/manifest").then((r) => r.json()).then((m) => request.get(`/blobs/${m.stops_hash}`)).then((r) => r.json());
  expect(stops.stops.A.some((e) => e.note === "Sidewalk work")).toBeTruthy();
  expect(stops.stops.B.some((e) => e.note === "Bench replaced")).toBeTruthy();
  expect(stops.stops.C.some((e) => e.note === "Tree trimming")).toBeFalsy();

  await held.getByTestId("sync-discard").click();
  await expect(page.getByTestId("sync-status")).toHaveCount(0);
});

test("service design: a publish made offline waits and is sent, and one refused at send time is kept with its reasons", async ({ page, context, request }) => {
  test.setTimeout(90_000);
  await controlled(page, "/service-design.html");
  await page.getByTestId("design-rail").getByRole("button", { name: "Plans" }).click();
  await expect(page.getByTestId("group-table").locator("tbody tr")).toHaveCount(2, { timeout: 20_000 });

  await context.setOffline(true);
  await page.reload();
  await page.getByTestId("design-rail").getByRole("button", { name: "Plans" }).click();
  await expect(page.getByTestId("group-table").locator("tbody tr")).toHaveCount(2, { timeout: 20_000 });
  await expect(page.getByTestId("sync-offline")).toBeVisible({ timeout: 15_000 });
  await page.getByTestId("fork-name").fill("Winter offline");
  await page.getByTestId("fork-date").fill("2026-11-23");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText("Forked");
  await page.keyboard.press("6");
  await page.getByTestId("publish").click();
  await expect(page.getByTestId("sync-waiting")).toContainText("1 change waiting to send", { timeout: 15_000 });
  // The server refuses this fork when sent: it is dated before the server's today.
  await page.keyboard.press("1");
  await page.getByTestId("fork-name").fill("Too late offline");
  await page.getByTestId("fork-date").fill("2026-07-01");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText("Forked");
  await page.keyboard.press("6");
  await page.getByTestId("publish").click();
  await expect(page.getByTestId("sync-waiting")).toContainText("2 changes waiting to send", { timeout: 15_000 });

  await context.setOffline(false);
  await page.getByTestId("sync-send").click({ timeout: 2_000 }).catch(() => {});
  const held = page.getByTestId("sync-held");
  await expect(held).toHaveCount(1, { timeout: 30_000 });
  await expect(held).toContainText('Publish "Too late offline"');
  await expect(held).toContainText("Not saved:");
  await expect(page.getByTestId("sync-waiting")).toHaveCount(0);
  const { plans_hash } = await (await request.get("/manifest")).json();
  const set = await (await request.get(`/blobs/${plans_hash}`)).json();
  expect(set.plans.map((p) => p.name)).toContain("Winter offline");
  expect(set.plans.map((p) => p.name)).not.toContain("Too late offline");
});

test("service change: it boots offline from the worker, on its plans, and a publish made offline is sent when the server answers", async ({ page, context, request }) => {
  test.setTimeout(90_000);
  await controlled(page, "/service-change.html");
  await page.getByRole("button", { name: /Plan & calendar/ }).click();
  await expect(page.getByTestId("plan-select")).toHaveValue(/./, { timeout: 20_000 });
  // The page opens on the pick that the service design test forked, whose
  // calendar starts 2026-11-23. A pick with no service in its period is refused.
  await page.getByTestId("fork-name").fill("Spring offline");
  await page.getByTestId("fork-date").fill("2026-12-07");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText('Started "Spring offline"');

  await context.setOffline(true);
  await page.reload();
  await expect(page.getByTestId("plan-select")).toHaveValue(/./, { timeout: 20_000 });
  await expect(page.getByTestId("sync-offline")).toBeVisible({ timeout: 15_000 });
  await page.getByRole("button", { name: /Proposal/ }).click();
  await page.getByTestId("publish").click();
  await expect(page.getByTestId("sync-waiting")).toContainText("1 change waiting to send", { timeout: 15_000 });

  await page.getByRole("button", { name: /Plan & calendar/ }).click();
  await page.getByTestId("fork-name").fill("Late offline");
  await page.getByTestId("fork-date").fill("2026-07-01");
  await page.getByRole("button", { name: "Fork" }).click();
  await expect(page.getByTestId("status")).toContainText('Started "Late offline"');
  await page.getByRole("button", { name: /Proposal/ }).click();
  await page.getByTestId("publish").click();
  await expect(page.getByTestId("sync-waiting")).toContainText("2 changes waiting to send", { timeout: 15_000 });

  await context.setOffline(false);
  await page.getByTestId("sync-send").click({ timeout: 2_000 }).catch(() => {});
  await expect(page.getByTestId("sync-waiting")).toHaveCount(0, { timeout: 30_000 });
  await expect(page.getByTestId("sync-held")).toContainText('Publish "Late offline"');
  const { plans_hash } = await (await request.get("/manifest")).json();
  const set = await (await request.get(`/blobs/${plans_hash}`)).json();
  expect(set.plans.map((p) => p.name)).toContain("Spring offline");
  expect(set.plans.map((p) => p.name)).not.toContain("Late offline");
});

test("the disruptions page: a note edit refused at send time is kept and shown with its reasons", async ({ page, context, request }) => {
  test.setTimeout(90_000);
  const posted = await request.post("/edits", { data: { kind: "edit_records", schema_version: 2, records: [{ ...cancel, id: "c-offline-2", cancel: { ...cancel.cancel, id: "cancel-t30-2" } }] } });
  expect(posted.ok()).toBeTruthy();
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await controlled(page, "/disruptions.html");
  await page.locator('[data-testid="route-card"][data-route="10"]').getByTestId("disruption-line").first().click();
  const detail = page.getByTestId("disruption-detail");
  await detail.getByTestId("note-box").getByLabel("Note").fill("Operator called in.");
  await detail.getByTestId("add-note").click();
  await expect(detail.getByTestId("detail-notes")).toContainText("Operator called in.", { timeout: 15_000 });
  const notes = await request.get("/manifest").then((r) => r.json()).then((m) => request.get(`/blobs/${m.notes_hash}`)).then((r) => r.json());
  const [id, held] = Object.entries(notes.notes).find(([, n]) => n.text === "Operator called in.");

  await context.setOffline(true);
  const row = detail.getByTestId("detail-notes").locator("li", { hasText: "Operator called in." });
  await row.getByRole("button", { name: "Edit" }).click();
  // One note is edited at a time: its box and Save are the only ones.
  await detail.getByTestId("detail-notes").getByRole("textbox").fill("Operator called in. Covered by the extra board.");
  await detail.getByTestId("detail-notes").getByRole("button", { name: "Save" }).click();
  await expect(page.getByTestId("sync-waiting")).toContainText("1 change waiting to send", { timeout: 15_000 });
  const other = await request.post("/logs/notes", { data: { key: "desk/9", ops: [{ cap: "notes", op: "write", args: { id, target: held.target, text: "Operator called in. Trip cancelled." } }] } });
  expect(other.ok()).toBeTruthy();

  await context.setOffline(false);
  await page.getByTestId("sync-send").click({ timeout: 2_000 }).catch(() => {});
  const refused = page.getByTestId("sync-held");
  await expect(refused).toHaveCount(1, { timeout: 30_000 });
  await expect(refused).toContainText("Edit note: Operator called in. Covered by the extra board.");
  await expect(refused).toContainText("Someone else changed this");
  const after = await request.get("/manifest").then((r) => r.json()).then((m) => request.get(`/blobs/${m.notes_hash}`)).then((r) => r.json());
  expect(after.notes[id].text).toBe("Operator called in. Trip cancelled.");
});
