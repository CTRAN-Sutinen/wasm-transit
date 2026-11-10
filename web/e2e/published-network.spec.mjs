// The published network (docs/ARCHITECTURE.md, server section): a
// server started with -feed imports its zip once and serves the
// network; every generated UI boots from it with no GTFS in the
// browser, and refuses a network that does not register under the
// hash the server pins. The rider pages read every published document
// by the hash the manifest pins (blobs/<hash>, gzipped bytes they never
// parse — the layout a static host serves too, D-131); the planner
// takes the whole answer (/network) to fork.
//
// Its own server (playwright.config.mjs, :8083) publishes the fixture
// feed and the walking fixture's streets (Station Rd through the
// station's bays) with the walk table over the two — the shared e2e
// server publishes none, the file-input path the UI specs walk. It
// runs no geocoder (its manifest lists no service: spots stay
// "Dropped pin"); the geocoder specs list one and answer /geocode
// themselves.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const fixtures = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");

test.use({
  baseURL: "http://127.0.0.1:8083",
  timezoneId: "UTC",
  geolocation: { latitude: 45.515, longitude: -73.515 },
  permissions: ["geolocation"],
});

async function live(page) {
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
  await page.route("**/edits", (route) => route.fulfill({ contentType: "application/json", body: '{"kind":"edit_records","schema_version":2,"records":[]}' }));
  await page.route(/\/rt\?url=/, (route) =>
    route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(path.join(fixtures, "sample-rt.pb")) }),
  );
}

/// The manifest's pins: what the page fetches by hash.
const pins = async (page) => (await page.request.get("/manifest")).json();

/// What the page asked the server for, by path.
function requests(page) {
  const paths = [];
  page.on("request", (r) => paths.push(new URL(r.url()).pathname));
  return paths;
}

test("ride now boots from the published network", async ({ page }) => {
  await live(page);
  const asked = requests(page);
  await page.goto("/ride-now.html");

  const first = page.getByTestId("nearby").getByTestId("stop-card").first();
  await expect(first).toContainText("Main Station", { timeout: 10_000 });
  await expect(first).toContainText("8:15");
  // The bytes, never the parsed answer — and the heading is the
  // network's name, which the directory answers.
  const m = await pins(page);
  expect(asked).toEqual(expect.arrayContaining([`/blobs/${m.network_hash}.gz`, `/blobs/${m.boundary_hash}`]));
  expect(asked).not.toContain("/network");
  const { network } = await (await page.request.get("/network")).json();
  await expect(page.locator("h1")).toHaveText(network.name);
});

test.describe("ride now walks the published streets", () => {
  // ~22 m north of Station Rd, west of the bays.
  test.use({ geolocation: { latitude: 45.5152, longitude: -73.5165 } });

  test("the nearby list ranks by walk and a trip starts at my location", async ({ page }) => {
    await live(page);
    const asked = requests(page);
    await page.goto("/ride-now.html");

    // The station first, at its nearest bay's walk along Station Rd
    // (77 s: streets `access`, directory `stops_near` by walk); the
    // stops no walk reaches follow by distance.
    const cards = page.getByTestId("nearby").getByTestId("stop-card");
    await expect(cards.first().getByTestId("walk")).toHaveText("1 min walk", { timeout: 10_000 });
    await expect(cards.first()).toContainText("Main Station");
    await expect(cards.nth(1).getByTestId("walk")).toHaveCount(0);
    const m = await pins(page);
    expect(asked).toEqual(expect.arrayContaining([`/blobs/${m.streets_hash}.gz`, `/blobs/${m.walks_hash}`]));

    // Directions to the station start at my location: the router
    // plans from it as a place, and walking there beats the 8:15 bus.
    await cards.first().getByRole("button").first().click();
    await page.getByTestId("rider-board").getByRole("button", { name: "Directions to here" }).click();
    await expect(page.getByLabel("From", { exact: true })).toHaveValue("My location");
    const plan = page.getByTestId("plan-now");
    await expect(plan).toContainText("min walk to Main Station");
    // The map draws a walk as the router answers it (`itinerary_paths`;
    // the path through a merged walk's via stops is pinned by cap-router's
    // `a_place_s_walk_draws_along_its_path`): to Bay B, my location, its
    // street (Station Rd), then the bay — along the streets, never a
    // straight line.
    const to = page.getByLabel("To", { exact: true });
    await to.fill("bay b");
    await page.getByRole("option", { name: /Bay B/ }).click();
    await expect(page.getByTestId("ride-map")).toHaveAttribute("data-legs", "1");
    await expect
      .poll(() => page.evaluate(() => window.__map.getSource("legs")?.serialize().data.features.map((f) => [f.properties.kind, f.geometry.coordinates])))
      .toEqual([["walk", [[-73.5165, 45.5152], [-73.5165, 45.515], [-73.5145, 45.515]]]]);
  });
});

/// Tap the ride map at a lon/lat: centered there first, so the spot
/// is clear of the panel.
async function tapMap(page, lon, lat) {
  await page.waitForFunction(() => window.__map?.loaded());
  const at = await page.evaluate(([lon, lat]) => {
    const map = window.__map;
    map.jumpTo({ center: [lon, lat], zoom: 17 });
    const r = map.getCanvas().getBoundingClientRect();
    const p = map.project([lon, lat]);
    return { x: Math.round(p.x + r.x), y: Math.round(p.y + r.y) };
  }, [lon, lat]);
  await page.mouse.click(at.x, at.y);
}

test.describe("ride now chooses a trip's end on the map", () => {
  test.use({ geolocation: { latitude: 45.5152, longitude: -73.5165 } });

  test("a spot tapped while choosing becomes the destination, walked to along the streets", async ({ page }) => {
    await live(page);
    await page.goto("/ride-now.html");
    await expect(page.getByTestId("nearby").getByTestId("walk").first()).toBeVisible({ timeout: 10_000 });

    await page.getByRole("button", { name: "Get directions" }).click();
    await page.getByRole("button", { name: "Start: my location" }).click();
    await page.getByRole("button", { name: "Destination: choose on map" }).click();
    await expect(page.getByTestId("pick-banner")).toContainText("Tap the map to set your destination");

    // East of the bays, on Station Rd.
    await tapMap(page, -73.5134, 45.5151);
    await expect(page.getByTestId("pick-banner")).toHaveCount(0);
    await expect(page.getByLabel("To", { exact: true })).toHaveValue("Dropped pin");
    await expect(page.getByTestId("plan-now")).toContainText("min walk to Dropped pin");
  });

  test("directions here, from a spot tapped on the home panel's map", async ({ page }) => {
    await live(page);
    await page.goto("/ride-now.html");
    await expect(page.getByTestId("nearby").getByTestId("walk").first()).toBeVisible({ timeout: 10_000 });

    await tapMap(page, -73.5134, 45.5151);
    await expect(page.getByTestId("nearby")).toContainText("Near this spot");
    await page.getByTestId("directions-here").click();
    await expect(page.getByLabel("From", { exact: true })).toHaveValue("My location");
    await expect(page.getByLabel("To", { exact: true })).toHaveValue("Dropped pin");
    await expect(page.getByTestId("plan-now")).toContainText("min walk to Dropped pin");
  });
});

/// The geocoder, as a deployment that runs one offers it: the
/// manifest lists it, and its /geocode proxy answers (Pelias GeoJSON)
/// an address east of the bays on Station Rd for any typed text, and
/// the address nearest any spot. The asked paths and queries are
/// kept.
async function geocoder(page) {
  const asked = [];
  await page.route("**/manifest", async (route) => {
    const manifest = await (await route.fetch()).json();
    manifest.services = { geocoder: { path: "/geocode" } };
    return route.fulfill({ contentType: "application/json", body: JSON.stringify(manifest) });
  });
  const feature = (name, lon, lat) => ({
    type: "Feature",
    geometry: { type: "Point", coordinates: [lon, lat] },
    properties: { gid: `openaddresses:address:${name}`, layer: "address", name, label: `${name}, Testville`, locality: "Testville" },
  });
  await page.route(/\/geocode\//, (route) => {
    const url = new URL(route.request().url());
    asked.push(`${url.pathname}?${[...url.searchParams.keys()].sort().join("&")}`);
    const features = url.pathname.endsWith("/reverse")
      ? [feature("210 Station Rd", -73.5133, 45.5151)]
      : [feature("200 Station Rd", -73.5134, 45.5151)];
    return route.fulfill({ contentType: "application/json", body: JSON.stringify({ type: "FeatureCollection", features }) });
  });
  return asked;
}

test.describe("ride now searches addresses through the geocoder", () => {
  test.use({ geolocation: { latitude: 45.5152, longitude: -73.5165 } });

  test("an address typed is a spot: what is near it, and directions there", async ({ page }) => {
    await live(page);
    const asked = await geocoder(page);
    await page.goto("/ride-now.html");
    await expect(page.getByTestId("nearby").getByTestId("walk").first()).toBeVisible({ timeout: 10_000 });

    // The search takes addresses once the server answers for a geocoder.
    const search = page.getByLabel("stop name");
    await expect(search).toHaveAttribute("placeholder", "Search for a stop or address");
    await search.fill("200 station rd");
    const listbox = page.getByRole("listbox");
    await expect(listbox).toContainText("Addresses and places");
    await expect(listbox.getByTestId("address-result")).toContainText("200 Station Rd");
    await expect(listbox.getByTestId("address-result")).toContainText("Testville");
    // No stop carries the words, but an address answered: no "no stop" hint.
    await expect(listbox).not.toContainText("No stop is named");
    // The page sends only what the rider did: the text and where the
    // map looks — the query policy is the deployment's.
    expect(asked).toContain("/geocode/autocomplete?focus.point.lat&focus.point.lon&text");
    await search.press("Enter");

    // The address is a spot, named as it was shown.
    await expect(page.getByTestId("nearby")).toContainText("Near 200 Station Rd");
    await page.getByTestId("directions-here").click();
    await expect(page.getByLabel("From", { exact: true })).toHaveValue("My location");
    await expect(page.getByLabel("To", { exact: true })).toHaveValue("200 Station Rd");
    await expect(page.getByTestId("plan-now")).toContainText("min walk to 200 Station Rd");
  });

  test("an address typed as a trip's end, and a dropped pin named by the geocoder", async ({ page }) => {
    await live(page);
    await geocoder(page);
    await page.goto("/ride-now.html");
    await expect(page.getByTestId("nearby").getByTestId("walk").first()).toBeVisible({ timeout: 10_000 });

    await page.getByRole("button", { name: "Get directions" }).click();
    await page.getByRole("button", { name: "Start: my location" }).click();
    const to = page.getByLabel("To", { exact: true });
    await expect(to).toHaveAttribute("placeholder", "To — a stop or address");
    await to.fill("200 station");
    await expect(page.getByTestId("address-result")).toContainText("200 Station Rd");
    await to.press("Enter");
    await expect(to).toHaveValue("200 Station Rd");
    await expect(page.getByTestId("plan-now")).toContainText("min walk to 200 Station Rd");

    // A spot tapped while choosing takes the nearest address's name.
    await page.getByRole("button", { name: "Destination: choose on map" }).click();
    await tapMap(page, -73.5133, 45.5151);
    await expect(to).toHaveValue("210 Station Rd");
    await expect(page.getByTestId("plan-now")).toContainText("min walk to 210 Station Rd");
  });
});

test("the stop display boots from the published network", async ({ page }) => {
  await live(page);
  const asked = requests(page);
  await page.goto("/stop-display.html?stop=A&layout=routes");

  await expect(page.getByTestId("display-stop")).toHaveText("Main & 1st", { timeout: 10_000 });
  const ten = page.locator('[data-testid="display-group"][data-route="10"]').first();
  await expect(ten.getByTestId("display-time").first()).toContainText("2 min late");
  expect(asked).toContain(`/blobs/${(await pins(page)).network_hash}.gz`);
  expect(asked).not.toContain("/network");
});

test("service design opens the published network as a plan", async ({ page }) => {
  await page.goto("/service-design.html");

  // The fixture's combinations that run together (D-55): WK alone and
  // SUN alone — HOL runs no trip, so it is in none.
  await expect(page.getByTestId("group-select").locator("option")).toHaveCount(2, { timeout: 10_000 });
  await page.getByTestId("design-rail").getByRole("button", { name: "Plans" }).click();
  await expect(page.getByTestId("group-table").locator("tbody tr")).toHaveCount(2);
  await expect(page.getByTestId("plan-list")).not.toContainText("the published network");
});

test("a network that does not register under the pinned hash is refused", async ({ page }) => {
  await live(page);
  // The manifest pins a wrong hash, and that blob answers the real
  // network's bytes: the module registers them under their own hash.
  const wrong = `sha256:${"0".repeat(64)}`;
  const real = (await (await page.request.get("/manifest")).json()).network_hash;
  await page.route(/\/manifest$/, async (route) => {
    const response = await route.fetch();
    await route.fulfill({ response, json: { ...(await response.json()), network_hash: wrong } });
  });
  await page.route(`**/blobs/${wrong}.gz`, async (route) => {
    const response = await page.request.get(`/blobs/${real}.gz`);
    await route.fulfill({ contentType: "application/gzip", body: await response.body() });
  });
  await page.goto("/ride-now.html");

  await expect(page.getByTestId("ride-status")).toContainText("the served network registers as", { timeout: 10_000 });
  await expect(page.getByTestId("nearby").getByTestId("stop-card")).toHaveCount(0);
});
