// Step 2, the service-change app: a fork for route 99, the new stops
// found on it, the route built, its trips blocked on every day type,
// compared, and published to take effect today.
//
//   node tools/ui-shot.mjs service-change --hash '#stage=calendar' \
//     --steps qa/route-99/2-service-change.mjs --out .qa/route-99/sc.png
//
// The route is built on the Route stage's New route task,
// its stops picked from the stops record (D-173).
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { STOPS, DATES, ROUTE, RUN_SECS, SERVICE } from "./route.mjs";
import { check, done, clean } from "./check.mjs";

const T = (p, id) => p.getByTestId(id);
const OUT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../.qa/route-99");
const FORK = "Route 99 — 2045 concept";

export default async function (page, { shot, status, settle }) {
  const stage = async (name) => {
    await page.getByRole("navigation", { name: "Stages" }).getByRole("button", { name: new RegExp(name) }).first().click();
    await settle();
  };
  const rows = T(page, "pick-row");
  const open = () => rows.filter({ has: page.locator(".tag.open") });
  await settle();

  // 2.1 the fork, picking today
  if (!(await rows.filter({ hasText: FORK }).count())) {
    await T(page, "fork-name").fill(FORK);
    await T(page, "fork-date").fill(DATES.today);
    await page.getByRole("button", { name: "Fork" }).click();
    await settle();
  } else if (!(await open().filter({ hasText: FORK }).count())) {
    await rows.filter({ hasText: FORK }).first().getByRole("button", { name: "Open" }).click();
    await settle();
  }
  check("2.1 the fork is the open draft", clean(await open().textContent()).includes(FORK), await status());

  // 2.2 + 2.3 a new route: the stops app's stops offered in
  // its picker (D-173), laid each way, its trips per service key
  await stage("Route");
  if (await T(page, "close-route").count()) { await T(page, "close-route").click(); await settle(); }
  await T(page, "new-route-start").click();
  await T(page, "new-route-id").waitFor({ timeout: 30000 });
  await T(page, "new-route-id").fill(ROUTE.id);
  await T(page, "new-route-name").fill(ROUTE.id);
  await T(page, "new-route-long-name").fill(ROUTE.long_name);
  await T(page, "new-route-next").click();
  await T(page, "new-route-way-0-name").fill("East Fourth Plain TC");
  await T(page, "new-route-way-0-run").fill(String(RUN_SECS / 60));
  const add = T(page, "new-route-way-0-add").getByRole("combobox");
  for (const [id, name] of STOPS) {
    await add.fill(name);
    await page.waitForTimeout(700);
    const offered = clean(await page.getByRole("listbox").textContent().catch(() => ""));
    check(`2.2 ${id} ${name} offered, from the stops record`, offered.includes(name) && offered.includes("stops record"), offered.slice(0, 120));
    await add.press("Enter");
    await page.waitForTimeout(300);
  }
  const laid = clean(await T(page, "new-route-way-0-stops").textContent());
  check("2.3 direction 1 runs the six stops in order", STOPS.every(([id], i) => laid.indexOf(id) >= 0 && (i === 0 || laid.indexOf(id) > laid.indexOf(STOPS[i - 1][0]))), laid.slice(0, 200));
  await T(page, "new-route-way-1-name").fill("Washougal TC");
  await T(page, "new-route-way-1-run").fill(String(RUN_SECS / 60));
  await shot("new-route-directions", { full: true });
  await T(page, "new-route-next").click();
  for (const [dir, offset] of [[0, 0], [1, 15]]) {
    const table = T(page, `new-route-trips-${dir}`);
    for (let k = 0; k < SERVICE.length; k++) {
      if ((await table.getByTestId("new-route-trip-row").count()) <= k) await T(page, `new-route-add-row-${dir}`).click();
      const [key, first, last, every] = SERVICE[k];
      const row = table.getByTestId("new-route-trip-row").nth(k);
      const hhmm = (h, m) => `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
      await row.getByLabel("Service key").selectOption(key);
      await row.getByLabel("First departure").fill(hhmm(first, offset));
      await row.getByLabel("Last departure").fill(dir === 1 && every === 30 ? hhmm(last - 1, 45) : hhmm(dir === 1 ? last - 1 : last, dir === 1 ? offset : 0));
      await row.getByLabel("Minutes between trips").fill(String(every));
    }
  }
  await shot("new-route-trips", { full: true });
  await T(page, "new-route-create").click();
  await settle();
  const made = await status();
  check("2.3 the route is created", made.includes("created"), made + " " + clean(await T(page, "new-route-problems").textContent().catch(() => "")));

  // 2.3 read back: route 099 on the Route stage
  await settle();
  const route = clean(await T(page, "stage-route").textContent());
  check("2.3 route 099 shows 57 weekday trips, 6:00–20:40", route.includes(ROUTE.long_name) && /TRIPS\s*57/i.test(route) && route.includes("6:00–20:40"), route.slice(0, 160));
  await shot("route-099", { full: true });

  // 2.4 the suggested blocks, adopted on every day type and combination
  await stage("Vehicles");
  const dayTypes = T(page, "day-types").getByRole("button");
  for (let i = 0; i < (await dayTypes.count()); i++) {
    await dayTypes.nth(i).click();
    await settle();
    const combos = T(page, "day-type-group");
    const values = (await combos.count()) ? await combos.locator("option").evaluateAll((os) => os.map((o) => o.value)) : [null];
    for (const v of values) {
      if (v !== null) { await combos.selectOption(v); await settle(); }
      if ((await T(page, "unblocked").count()) && (await T(page, "adopt-blocks").isEnabled())) {
        await T(page, "adopt-blocks").click();
        await settle();
      }
    }
  }
  await shot("vehicles");

  // 2.5 the proposal against the plan in force; 2.6 published
  await stage("Proposal");
  const base = T(page, "base-select");
  await base.selectOption({ label: "C-TRAN" });
  await settle();
  await page.waitForTimeout(2000);
  const ready = clean(await T(page, "readiness").textContent());
  check("2.4 every route 99 trip is on a block", !/unassigned trips[^!]*099/.test(ready), ready.slice(0, 300));
  check("2.5 readiness: no issues", (await page.getByText(/^0 issues/).count()) > 0 || /Ready/.test(ready), ready.slice(0, 120));
  check("2.5 readiness: no stop warnings", !/not in service/i.test(ready));
  const score = clean(await page.locator(".two > div").first().textContent());
  check("2.5 the scorecard names route 099", score.includes(ROUTE.id), score.slice(0, 160));
  await shot("proposal", { full: true });
  await T(page, "publish").click();
  await page.waitForTimeout(3000);
  await settle();
  const said = await status();
  check(`2.6 published, taking effect ${DATES.today}`, said.startsWith("Published") && said.includes(DATES.today), said);
  const feed = await fetch(`http://127.0.0.1:8091/feed?date=${DATES.today}`).then((r) => (r.ok ? r.arrayBuffer() : null)).catch(() => null);
  fs.mkdirSync(OUT, { recursive: true });
  const file = path.join(OUT, "feed-today.zip");
  if (feed) fs.writeFileSync(file, Buffer.from(feed));
  const routes = feed ? execFileSync("unzip", ["-p", file, "routes.txt"]).toString() : "";
  check("2.6 the feed in force today carries route 99", routes.includes(ROUTE.long_name));
  done("2. service change");
}
