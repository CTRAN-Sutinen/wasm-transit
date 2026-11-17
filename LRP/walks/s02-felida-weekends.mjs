// Session 2 — Growth: Saturday and Sunday service on 9 Felida, a route
// that runs weekdays only (LRP/SESSIONS.md). Builds on session 1: fork
// its published pick, open route 9 on Saturday, and give it a day.
// Blocked today by QAF-118 (no way to start service on a day type where
// a route runs no trip); the walk pins it.
import { check, done, helpers, sundayAfter } from "./lib.mjs";

const PLAN = "2045 Growth — Felida weekends";
const PICK = sundayAfter(4); // inside the test feed (it ends 2027-01-09)

export default async function (page, { shot, status, settle }) {
  const h = helpers(page, settle);
  await settle();
  await h.openPlan("2045 Growth — evening span 9, 92");
  await h.fork(PLAN, PICK);
  check(`2.1 forked "${PLAN}" from session 1's pick, picking ${PICK}`, (await status()).includes(`Started "${PLAN}" from 2045 Growth — evening span 9, 92`), await status());

  await page.getByRole("button", { name: "Sat", exact: true }).click();
  await settle();
  await h.stage("Route");
  await h.route("009");
  await h.direction("to 139th Street & 10th Avenue");
  await h.span("6:00", "22:15");
  const said = await status();
  await shot("009-saturday", { full: true });
  check("2.2 route 9 runs Saturdays 6:00–22:15 (QAF-118)", (await h.spanNow()).includes("Now 6:00 to 22:15"), said);
  done("session 2");
}
