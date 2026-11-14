// Step 1, the stops app: each route 99 stop through the planning
// process — planned with its installation request, the request
// submitted, its work order opened, permitted, built, the request
// completed, and the stop record updated from it: in service today.
//
//   node tools/ui-shot.mjs stops --fresh --hash "#on=$(date +%F)" \
//     --steps qa/route-99/1-stops.mjs --out .qa/route-99/stops.png
import { STOPS, DATES } from "./route.mjs";
import { check, done, until, clean } from "./check.mjs";

const T = (p, id) => p.getByTestId(id);

async function oneStop(page, [id, name, lat, lon], shot) {
  const visible = (tid) => () => T(page, tid).first().isVisible();
  const status = async () => clean(await T(page, "status").textContent());
  const track = (s) => async () => /now/.test((await T(page, `request-track-${s}`).getAttribute("class")) ?? "");

  await page.goto(page.url().replace(/#.*/, `#on=${DATES.today}`));
  await until(page, async () => !(await status()).startsWith("…"), 60000);

  // 1. a new stop, planned, at the typed point, its request next
  await T(page, "new-stop-start").click();
  await T(page, "new-point-lat").fill(String(lat));
  await T(page, "new-point-lon").fill(String(lon));
  await T(page, "new-next").click();
  await T(page, "new-id").fill(id);
  await T(page, "new-name").fill(name);
  await T(page, "new-code").fill(id);
  await T(page, "new-start-planned").click();
  await T(page, "new-on").fill(DATES.planned);
  await T(page, "new-note").fill("Route 99 (2045 plan concept): Washougal – Camas – East Fourth Plain");
  await T(page, "new-save").click();
  if (!check(`${id} created, its request page open`, await until(page, visible("request-view")), await status())) return;
  check(`${id} request job is New stop`, (await T(page, "request-job-new").getAttribute("aria-pressed")) === "true");

  // 2. the installation request, submitted
  await T(page, "request-routes").fill("99");
  await T(page, "request-on").fill(DATES.planned);
  const rid = await T(page, "request-id").inputValue();
  await T(page, "request-submit").click();
  check(`${id} request ${rid} submitted`, await until(page, track("submitted")), await status());

  // 3. its work order, opened through the adapter
  await T(page, "request-wo-open").click();
  check(`${id} work order opened`, await until(page, visible("request-complete")), await status());
  await T(page, "request-back").click();
  await until(page, visible("stop-card"));

  // 4. permitted, then built, each carrying the request
  for (const kind of ["permitted", "built"]) {
    await T(page, "action-change").click();
    await T(page, `action-${kind}`).click();
    await T(page, "form-on").fill(DATES[kind]);
    await T(page, "form-request").selectOption(rid).catch(() => {});
    await T(page, "form-save").click();
    check(`${id} ${kind} ${DATES[kind]}`, await until(page, async () => (await T(page, "stop-status").textContent()).toLowerCase().includes(kind)), await status());
  }

  // 5. the request completed; 6. the stop record updated from it
  await T(page, "request-row").first().click();
  await until(page, visible("request-complete"));
  await T(page, "request-complete").click();
  await T(page, "request-completed-on").fill(DATES.built);
  await T(page, "request-save").click();
  check(`${id} request completed`, await until(page, track("completed")), await status());
  await T(page, "request-record").click();
  await until(page, visible("change-choices"));
  await T(page, "choose-in_service").click();
  await T(page, "form-on").fill(DATES.in_service);
  await T(page, "form-save").click();
  const inService = await until(page, async () => (await T(page, "stop-status").textContent()).toLowerCase().includes("in service"));
  check(`${id} in service ${DATES.in_service}`, inService, clean(await T(page, "stop-status").textContent()));
  check(`${id} history has four events`, (await T(page, "stop-event").count()) === 4);
  await shot(`${id}-in-service`);
}

export default async function (page, { shot }) {
  for (const stop of STOPS) await oneStop(page, stop, shot);
  // the work across every stop: six completed New stop requests
  await page.goto(page.url().replace(/#.*/, `#view=work&on=${DATES.today}&work=completed`));
  await page.waitForTimeout(3000);
  const rows = clean(await page.locator("main, body").first().textContent());
  check("Work view lists the six completed requests", STOPS.every(([id]) => rows.includes(`${id}-`)));
  await shot("work");
  done("1. stops");
}
