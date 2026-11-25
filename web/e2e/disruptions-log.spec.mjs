// The disruptions log in a browser (D-190; docs/FORMATS.md, "The logs"). The
// page's protocol is unchanged: GET /edits serves the fold of the log.
// The time is Friday 2026-10-23 at 07:55 in America/Montreal. The detour takes
// the 10 from C.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");
const server = "http://127.0.0.1:8084";

test.use({ baseURL: server, timezoneId: "UTC" });

const detour = {
  kind: "edit_records",
  schema_version: 2,
  records: [
    {
      id: "e-disruptions-log",
      rev: 1,
      status: "accepted",
      updated_at: 1,
      diversion: {
        id: "d-disruptions-log",
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
  ],
};

test("a detour written to the disruptions log reaches the stop display", async ({ page, request }) => {
  const posted = await request.post(`${server}/edits`, { data: detour });
  expect(posted.status()).toBe(200);
  const fold = await (await request.get(`${server}/edits`)).json();
  expect(fold.records.map((r) => r.id)).toContain("e-disruptions-log");
  const log = await (await request.get(`${server}/logs/disruptions?since=0`)).json();
  const entry = log.entries.map((e) => e.entry).find((e) => e.changes.some((c) => c.id === "e-disruptions-log"));
  expect(entry.kind).toBe("log_entry");
  expect(entry.intent.op).toBe("POST /edits");
  expect(log.document).toBe(posted.headers()["etag"].replaceAll('"', ""));

  await request.post(`${server}/edits`, { data: detour });
  const again = await (await request.get(`${server}/logs/disruptions?since=0`)).json();
  expect(again.head).toBe(log.head);

  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
  await page.goto("/stop-display.html?stop=C&layout=route");
  await expect(page.getByTestId("display-stop")).toHaveText("Main & 3rd", { timeout: 15_000 });
  await expect(
    page
      .locator('[data-testid="display-group"][data-route="10"]')
      .filter({ has: page.getByTestId("display-dest").getByText("Downtown", { exact: true }) })
      .getByTestId("display-none"),
  ).toHaveText("Detoured — not serving this stop");
});
