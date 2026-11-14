// Step 2, the service-change app: a fork for route 99, the new stops
// found on it, the route built, its trips blocked on every day type,
// compared, and published to take effect today.
//
//   node tools/ui-shot.mjs service-change --hash '#stage=calendar' \
//     --steps qa/route-99/2-service-change.mjs --out .qa/route-99/sc.png
//
// Until QAF-112 is fixed the UI cannot build the route: steps 2.2 and
// 2.3 are checked (and fail), then the route is imported as a draft from
// the runner's zip (`node qa/route-99/build-route.mjs`) so the rest of
// the test runs.
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { STOPS, DATES, ROUTE } from "./route.mjs";
import { check, done, clean } from "./check.mjs";

const T = (p, id) => p.getByTestId(id);
const ZIP = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../.qa/route-99/ctran-route99.zip");
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

  // 2.2 the stops app's new stops, on the fork
  await stage("Stops");
  const find = page.getByPlaceholder("Find a stop by name or id…");
  for (const [id, name] of [STOPS[5], STOPS[0]]) {
    await find.fill(name);
    await page.waitForTimeout(800);
    const listed = clean(await page.getByRole("listbox").textContent().catch(() => ""));
    check(`2.2 ${id} ${name} found on the fork`, listed.includes(name), listed.slice(0, 120));
  }
  await find.fill("");
  await shot("stops-stage");

  // 2.3 a control that makes a route
  await stage("Route");
  const make = page.getByRole("button", { name: /new route|add route/i });
  check("2.3 the UI can create a route", await make.count());
  // (until it can: the runner's network, imported as a draft)
  const built = rows.filter({ hasText: "(built)" });
  if (!(await built.count())) {
    await stage("Plan");
    await T(page, "feed-input").setInputFiles(ZIP);
    await page.waitForTimeout(2000);
    await settle();
    await open().getByTestId("rename-plan").click();
    await page.getByLabel(/^New name for/).fill(`${FORK} (built)`);
    await page.getByLabel(/^New name for/).press("Enter");
    await settle();
    await built.getByTestId("date-plan").click();
    await page.getByLabel(/^New pick date for/).fill(DATES.today);
    await page.getByLabel(/^New pick date for/).press("Enter");
    await settle();
  } else if (!(await built.locator(".tag.open").count())) {
    await built.getByRole("button", { name: "Open" }).click();
    await settle();
  }
  check("2.3 the built draft is open and picks today", clean(await open().textContent()).includes(`(built)`) && clean(await open().textContent()).includes(DATES.today), clean(await open().textContent()));

  // 2.3 read back: route 099 on the Route stage
  const box = page.getByPlaceholder("Open a route…");
  await box.fill(ROUTE.id);
  await page.waitForTimeout(600);
  await box.press("Enter");
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
  const file = ZIP.replace("ctran-route99.zip", "feed-today.zip");
  if (feed) fs.writeFileSync(file, Buffer.from(feed));
  const routes = feed ? execFileSync("unzip", ["-p", file, "routes.txt"]).toString() : "";
  check("2.6 the feed in force today carries route 99", routes.includes(ROUTE.long_name));
  done("2. service change");
}
