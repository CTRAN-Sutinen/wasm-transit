// Step 4, the stop display: each end of route 99 in the layouts a
// screen there would use — the next three 099 departures, minutes to
// each, and no bay tag at a stop that is not a station.
//
//   node tools/ui-shot.mjs stop-display --size 1920x1080 \
//     --steps qa/route-99/4-stop-display.mjs --out .qa/route-99/display.png
import { ROUTE } from "./route.mjs";
import { check, done, until, clean } from "./check.mjs";

const SCREENS = [
  ["9906", "route", "Washougal Transit Center", "East Fourth Plain TC"],
  ["9901", "routes", "East Fourth Plain Transit Center", "Washougal TC"],
  ["9906", "center", "Washougal Transit Center", "East Fourth Plain TC"],
];

export default async function (page, { shot }) {
  const base = page.url().replace(/[?#].*/, "");
  for (const [stop, layout, name, to] of SCREENS) {
    await page.goto(`${base}?stop=${stop}&layout=${layout}`);
    const body = page.locator("body");
    await until(page, async () => clean(await body.textContent()).includes(to), 30000);
    const screen = clean(await body.textContent());
    const times = (screen.match(/\d+ min|\d{1,2}:\d{2} [ap]m Scheduled|\d{1,2}:\d{2} [ap]m Live/g) ?? []).length;
    check(`4 ${stop} ${layout}: names the stop`, screen.startsWith(name), screen.slice(0, 60));
    check(`4 ${stop} ${layout}: 099 to ${to}, three departures`, screen.includes(`${ROUTE.id} ${to}`) && times >= 3, screen.slice(0, 200));
    // the stop's name once, in the header — a bay tag would repeat it
    if (layout === "center") check(`4 ${stop} center: no bay tag at a stop that is not a station`, screen.split(name).length - 1 === 1, `${screen.split(name).length - 1} times`);
    await shot(`${stop}-${layout}`);
  }
  done("4. stop display");
}
