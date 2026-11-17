// What every LRP session walk shares: PASS/FAIL checks, the pick date,
// the stage and route openers, and the server the walk publishes to.
// The walks run under tools/ui-shot.mjs (LRP/run.sh).

const results = [];

export function check(name, ok, saw = "") {
  results.push({ name, ok: !!ok });
  console.log(`  ${ok ? "PASS" : "FAIL"} ${name}${saw ? ` — ${saw}` : ""}`);
  return !!ok;
}

export function done(step) {
  const failed = results.filter((r) => !r.ok);
  console.log(`  ${step}: ${results.length - failed.length}/${results.length} passed`);
  if (failed.length) throw new Error(`${step}: failed ${failed.map((f) => f.name).join("; ")}`);
}

export const clean = (s) => (s ?? "").replace(/\s+/g, " ").trim();

/// The sync server the page talks to (LRP/run.sh sets it; `just qa`'s
/// by default).
export const SERVER = process.env.LRP_SERVER ?? "http://127.0.0.1:8091";

function iso(d) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
/// The Sunday at least `weeks` weeks out: a pick date, as a planner
/// sets one. The C-TRAN test feed runs through 2027-01-09, so a session
/// run later needs a newer feed.
export function sundayAfter(weeks) {
  const d = new Date();
  d.setDate(d.getDate() + weeks * 7);
  d.setDate(d.getDate() + ((7 - d.getDay()) % 7));
  return iso(d);
}
export function addDays(date, n) {
  const d = new Date(`${date}T12:00:00`);
  d.setDate(d.getDate() + n);
  return iso(d);
}

export function helpers(page, settle) {
  const T = (id) => page.getByTestId(id);
  return {
    T,
    stage: async (name) => {
      await page.getByRole("navigation", { name: "Stages" }).getByRole("button", { name: new RegExp(name) }).first().click();
      await settle();
    },
    /// Fork the open plan as `name`, picking on `date`; the fork opens.
    fork: async (name, date) => {
      await T("fork-name").fill(name);
      await T("fork-date").fill(date);
      await page.getByRole("button", { name: "Fork" }).click();
      await settle();
    },
    /// Open a plan already on the device by its name (Plan & calendar).
    openPlan: async (name) => {
      const row = T("pick-row").filter({ hasText: name }).first();
      if (await row.getByRole("button", { name: "Open" }).count()) {
        await row.getByRole("button", { name: "Open" }).click();
        await settle();
      }
    },
    route: async (id) => {
      const box = page.getByPlaceholder("Open a route…");
      await box.fill(id);
      await page.waitForTimeout(600);
      await box.press("Enter");
      await settle();
    },
    direction: async (name) => {
      await page.getByRole("button", { name, exact: true }).click();
      await settle();
      const edit = page.getByRole("button", { name: "Edit", exact: true });
      if (await edit.count()) { await edit.click(); await settle(); }
    },
    /// The span form: fill only the ends given, Apply; the status after.
    span: async (first, last) => {
      const f = T("gesture-span");
      if (first) await f.getByLabel("first trip").fill(first);
      if (last) await f.getByLabel("last trip").fill(last);
      await f.getByRole("button", { name: "Apply" }).click();
      await settle();
    },
    spanNow: async () => clean(await T("span-now").textContent()),
    /// The trip list's starts, in order.
    starts: async () => {
      const rows = page.locator("tr");
      const out = [];
      for (let i = 0; i < (await rows.count()); i++) {
        const m = clean(await rows.nth(i).textContent()).match(/^(\d{1,2}:\d{2})/);
        if (m) out.push(m[1]);
      }
      return out;
    },
  };
}

/// The served feed on `date` (`GET /feed`): routes.txt and trips.txt
/// as rows of cells, or null.
export async function servedFeed(date) {
  const { execFileSync } = await import("node:child_process");
  const fs = await import("node:fs");
  const os = await import("node:os");
  const path = await import("node:path");
  const res = await fetch(`${SERVER}/feed?date=${date}`).catch(() => null);
  if (!res?.ok) return null;
  const file = path.join(os.tmpdir(), `lrp-feed-${date}.zip`);
  fs.writeFileSync(file, Buffer.from(await res.arrayBuffer()));
  const read = (name) => execFileSync("unzip", ["-p", file, name]).toString().replace(/^﻿/, "").split(/\r?\n/).filter(Boolean).map((l) => l.split(","));
  return { routes: read("routes.txt"), trips: read("trips.txt") };
}
