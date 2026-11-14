// Step 3, ride now: Washougal TC's board, a trip to East Fourth Plain
// TC leaving now and on the last bus, and route 099 under Routes.
//
//   node tools/ui-shot.mjs ride-now --steps qa/route-99/3-ride-now.mjs \
//     --out .qa/route-99/ride.png
import { ROUTE } from "./route.mjs";
import { check, done, until, clean } from "./check.mjs";

const T = (p, id) => p.getByTestId(id);

async function pick(page, field, text) {
  const input = page.getByLabel(field, { exact: true });
  await input.fill(text);
  await until(page, async () => (await page.getByRole("option").count()) > 0, 5000);
  const first = clean(await page.getByRole("option").first().textContent().catch(() => ""));
  await input.press("Enter");
  return first;
}

export default async function (page, { shot }) {
  await page.getByLabel("stop name").waitFor({ timeout: 60000 });
  await page.waitForTimeout(2000);

  // 3.1 the stop by name, and its board
  const first = await pick(page, "stop name", "Washougal Transit");
  check("3.1 Washougal Transit Center #9906 is the first answer", first.includes("Washougal Transit Center") && first.includes("9906"), first);
  const board = T(page, "rider-board");
  await until(page, async () => clean(await board.textContent()).includes(ROUTE.id));
  const rows = clean(await board.textContent());
  check("3.1 its board lists 099 to East Fourth Plain TC", rows.includes(`${ROUTE.id} East Fourth Plain TC`), rows.slice(0, 200));
  await shot("board");

  // 3.2 directions to East Fourth Plain TC, leaving now
  await board.getByRole("button", { name: "Directions from here" }).click();
  await pick(page, "To", "East Fourth Plain Transit Center");
  const trip = T(page, "trip-plan");
  await until(page, async () => (await trip.getByTestId("itinerary").count()) > 0 || /Couldn/.test(await trip.textContent()), 20000);
  const now = clean(await trip.textContent());
  check("3.2 one ride on 099, 40 min", (await trip.getByTestId("itinerary").count()) > 0 && now.includes(ROUTE.id) && clean(await trip.getByTestId("duration").textContent().catch(() => "")) === "40 min", now.slice(0, 240));
  check("3.2 the ride is drawn on the map", (await T(page, "ride-map").getAttribute("data-legs")) === "1");
  check("3.2 nothing a rider cannot read (no hashes)", !/sha256:/.test(now));
  await shot("leave-now");

  // 3.3 the last bus
  await trip.getByRole("radio", { name: "Last bus" }).click();
  await page.waitForTimeout(2500);
  const last = clean(await trip.textContent());
  check("3.3 the last bus is the 8:00 pm 099", (await trip.getByTestId("last-bus").count()) > 0 && last.includes("8:00"), last.slice(0, 240));
  await shot("last-bus");

  // 3.4 the route under Routes
  await trip.getByRole("button", { name: "back" }).click().catch(() => {});
  await page.getByRole("button", { name: /^Routes$/ }).first().click().catch(() => {});
  await page.waitForTimeout(1500);
  const r = page.getByRole("button", { name: new RegExp(ROUTE.id) }).first();
  const listed = await r.count();
  if (listed) { await r.click(); await page.waitForTimeout(2000); }
  const view = clean(await page.locator("body").textContent());
  check("3.4 Routes opens 099 with both directions", listed && view.includes("East Fourth Plain") && view.includes("Washougal"));
  await shot("route");
  done("3. ride now");
}
