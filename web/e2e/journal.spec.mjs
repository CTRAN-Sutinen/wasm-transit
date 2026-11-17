// The page's journal (src/bus.worker.ts; tools/journal.mjs): with
// `?journal`, the engine journals every module call as hashes, and the
// runner replays the page's journal to the same output hashes.
//
// The stop display on the fixture feed and its trip updates, at stop A
// (docs/architecture/web.md, "The journal"):
//
//   1. On the page's own linked assembly, every call answers the same.
//   2. With --single, each capability runs on its own module. The live
//      document never left the page's assembly (QAF-49), so the
//      board's calls that read it cannot repeat, and replay says so.
//      What the page's assembly held (evict's answer) is not compared
//      on the split instances. Nothing answers differently.

import { test, expect } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const here = path.dirname(fileURLToPath(import.meta.url));
const fixtures = path.join(here, "fixtures");
const runner = path.join(here, "..", "..", "tools", "caps-run.mjs");

test.use({ timezoneId: "UTC" });

/// The runner's replay of a journal file: its report, whatever its exit.
function replay(file, ...args) {
  try {
    return JSON.parse(execFileSync("node", [runner, "replay", file, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }));
  } catch (error) {
    return JSON.parse(error.stdout);
  }
}

test("a page's journal replays through the runner", async ({ page }, testInfo) => {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await page.route("**/edits", (route) =>
    route.fulfill({ contentType: "application/json", body: JSON.stringify({ kind: "edit_records", schema_version: 2, records: [] }) }),
  );
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
  await page.goto("/stop-display.html?journal&stop=A&layout=routes");
  await page.getByLabel("Agency GTFS zip").setInputFiles(path.join(fixtures, "sample-gtfs.zip"));
  await expect(page.getByTestId("display-live")).toContainText("Live", { timeout: 10_000 });
  await expect(page.locator('[data-testid="display-group"][data-route="10"]').getByTestId("display-time").first()).toContainText("7 min");

  const bundle = await page.evaluate(() => globalThis.transitJournal());
  const lines = bundle.journal.trim().split("\n").map((l) => JSON.parse(l));
  expect(lines.length).toBeGreaterThan(5);
  // Hashes only: no line carries a document.
  expect(lines.every((l) => /^sha256:/.test(l.request) && /^sha256:/.test(l.output))).toBe(true);
  expect(lines.some((l) => l.cap === "board" && l.op === "grouped_board")).toBe(true);
  const file = testInfo.outputPath("journal.json");
  fs.writeFileSync(file, JSON.stringify(bundle));

  // 1. On the page's assembly: every call answers the same.
  const linked = replay(file);
  expect(linked.differ).toEqual([]);
  expect(linked.cannot).toEqual([]);
  expect(linked.same).toBe(lines.length);
  expect(linked.modules.board.replayed).toEqual(linked.modules.board.recorded);

  // 2. On single capabilities: the live document stayed in the page's
  // assembly, and replay says which calls it cannot repeat.
  const single = replay(file, "--single");
  expect(single.differ).toEqual([]);
  expect(single.cannot.length).toBeGreaterThan(0);
  expect(single.cannot.every((c) => c.cap === "board" && /^live document /.test(c.why))).toBe(true);
  expect(single.same + single.unchecked + single.cannot.length).toBe(lines.length);
});
