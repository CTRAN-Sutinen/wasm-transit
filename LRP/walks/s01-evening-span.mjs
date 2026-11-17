// Session 1 — Growth: weekday evening span on 9 Felida and 92
// Camas/Washougal (LRP/SESSIONS.md). Fork the plan in force, move the
// span ends on each direction, put the new trips on blocks, compare,
// publish. Starts from a fresh server and an empty profile.
import { check, done, clean, helpers, sundayAfter, addDays, servedFeed } from "./lib.mjs";

const PLAN = "2045 Growth — evening span 9, 92";
const PICK = sundayAfter(2);

export default async function (page, { shot, status, settle }) {
  const h = helpers(page, settle);
  const { T } = h;
  await settle();

  // 1.1 the fork
  await h.fork(PLAN, PICK);
  check(`1.1 forked "${PLAN}", picking ${PICK}`, (await status()).includes(`Started "${PLAN}"`), await status());

  // 1.2 route 92: to Camas & Washougal until 12:30 am, back until midnight
  await h.stage("Route");
  await h.route("092");
  await h.direction("to Camas & Washougal");
  await h.span(null, "24:30");
  check("1.2 092 to Camas & Washougal runs to 24:30", (await h.spanNow()).includes("to 24:30"), await h.spanNow());
  const camas = await h.starts();
  check("1.2 three trips added after 21:33 (even steps to the bound)", camas.slice(-3).join(" ") === "22:32 23:31 24:30", camas.slice(-4).join(" "));
  await h.direction("to Fisher's Landing Transit Center");
  await h.span(null, "24:00");
  check("1.2 092 to Fisher's Landing runs to 24:00", (await h.spanNow()).includes("to 24:00"), await h.spanNow());
  await shot("092", { full: true });

  // 1.3 route 9: 6:00 to about 10:15 pm each way
  await h.stage("Route");
  await h.route("009");
  await h.direction("to 139th Street & 10th Avenue");
  await h.span("6:00", "22:15");
  check("1.3 009 to 139th St runs 6:00–22:15", (await h.spanNow()).includes("Now 6:00 to 22:15"), await h.spanNow());
  await h.direction("to 99th Street Transit Center");
  await h.span("6:00", "21:45");
  const now = await h.spanNow();
  const said = await status();
  check("1.3 009 to 99th St TC runs to 21:45", now.includes("to 21:45"), now);
  // QAF-119: a bound within half a headway of the edge is dropped, and
  // the status says only "Changed".
  check("1.3 the 6:00 first trip toward 99th St TC applies, or the page says why not (QAF-119)",
    now.includes("Now 6:00") || /first|half/i.test(said), `${now} / ${said}`);
  await shot("009", { full: true });

  // 1.4 Vehicles & hours: the new trips on blocks, every day type
  await h.stage("Vehicles");
  const dayTypes = T("day-types").getByRole("button");
  let adopted = 0;
  for (let i = 0; i < (await dayTypes.count()); i++) {
    await dayTypes.nth(i).click();
    await settle();
    const combos = T("day-type-group");
    const values = (await combos.count()) ? await combos.locator("option").evaluateAll((os) => os.map((o) => o.value)) : [null];
    for (const v of values) {
      if (v !== null) { await combos.selectOption(v); await settle(); }
      if ((await T("unblocked").count()) && (await T("adopt-blocks").isEnabled())) {
        console.log("   suggested:", clean(await T("unblocked").textContent()).slice(0, 240));
        await shot(`suggested-${i}`);
        await T("adopt-blocks").click();
        await settle();
        adopted++;
      }
    }
  }
  check("1.4 the suggested blocks adopted", adopted > 0 && (await status()).includes("no issues"), await status());

  // 1.5 the proposal against the plan in force
  await h.stage("Proposal");
  await T("base-select").selectOption({ label: "C-TRAN" });
  await settle();
  await page.waitForTimeout(2000);
  const cards = clean(await page.locator("body").textContent());
  check("1.5 weekdays: 13 more trips, routes 009 and 092 changed", /1,773\s*→\s*1,786/.test(cards) && cards.includes("009") && cards.includes("092"), cards.slice(0, 220));
  check("1.5 readiness: no issues", clean(await T("readiness").textContent()).includes("No issues"));
  await shot("proposal", { full: true });

  // 1.6 published; the feed after the pick carries the trips
  await T("publish").click();
  await page.waitForTimeout(4000);
  await settle();
  check(`1.6 published, taking effect ${PICK}`, (await status()).startsWith(`Published "${PLAN}"`) && (await status()).includes(PICK), await status());
  const count = (feed) => feed?.trips.filter((r) => r[0] === "092" || r[0] === "009").length ?? -1;
  const before = await servedFeed(addDays(PICK, -2));
  const after = await servedFeed(addDays(PICK, 1));
  check("1.6 the Monday after the pick runs 13 more trips on 9 and 92 than the Friday before", count(after) - count(before) === 13, `${count(before)} → ${count(after)}`);
  done("session 1");
}
