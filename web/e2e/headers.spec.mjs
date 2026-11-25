// The headers a deployment sends, seen from a browser (server/headers.go;
// docs/units/web-conformance.md, scope 2). The Go tests pin the rules, and this
// spec pins what arrives and that each rider page runs under its policy. A
// violation means the policy is too tight or the page reads an unknown origin.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const here = path.dirname(fileURLToPath(import.meta.url));
const fixtures = path.join(here, "fixtures");
const dist = path.join(here, "..", "dist");

test.use({
  baseURL: "http://127.0.0.1:8083",
  timezoneId: "UTC",
  geolocation: { latitude: 45.515, longitude: -73.515 },
  permissions: ["geolocation"],
});

const first = (dir, ext) => fs.readdirSync(path.join(dist, dir)).find((f) => f.endsWith(ext));

test("the server sends the baseline, a policy on each rider page, and gzip", async ({ request }) => {
  for (const url of ["/", "/ride-now.html", "/stop-display.html", "/live-feeds.json"]) {
    const h = (await request.get(url)).headers();
    expect(h["x-content-type-options"], url).toBe("nosniff");
    expect(h["referrer-policy"], url).toBe("strict-origin-when-cross-origin");
  }
  expect((await request.get("/")).headers()["content-security-policy"]).toBeUndefined();

  const ride = (await request.get("/ride-now.html")).headers();
  expect(ride["content-security-policy"]).toContain("'wasm-unsafe-eval'");
  expect(ride["content-security-policy"]).toContain("frame-ancestors 'none'");
  expect(ride["permissions-policy"]).toContain("geolocation=(self)");
  const display = (await request.get("/stop-display.html")).headers();
  expect(display["permissions-policy"]).toContain("geolocation=()");

  // The big files travel compressed; the documents the modules decode
  // themselves do not (application/gzip).
  const js = `/assets/${first("assets", ".js")}`;
  const wasm = `/caps/${first("caps", ".wasm")}`;
  for (const url of [js, wasm, "/ride-now.html"]) {
    const r = await request.get(url, { headers: { "Accept-Encoding": "gzip" } });
    expect(r.headers()["content-encoding"], url).toBe("gzip");
    expect(r.headers()["vary"], url).toContain("Accept-Encoding");
  }
  const m = await (await request.get("/manifest")).json();
  const blob = await request.get(`/blobs/${m.network_hash}.gz`, { headers: { "Accept-Encoding": "gzip" } });
  expect(blob.headers()["content-encoding"]).toBeUndefined();
  expect(blob.headers()["content-type"]).toBe("application/gzip");
});

async function watchPolicy(page) {
  const messages = [];
  page.on("console", (m) => {
    if (/content security policy|violates the following/i.test(m.text())) messages.push(m.text());
  });
  await page.addInitScript(() => {
    window.__violations = [];
    document.addEventListener("securitypolicyviolation", (e) => window.__violations.push(`${e.violatedDirective}: ${e.blockedURI}`));
  });
  return async () => [...messages, ...(await page.evaluate(() => window.__violations))];
}

async function feeds(page) {
  const body = fs.readFileSync(path.join(fixtures, "sample-rt.pb"));
  await page.route(/\/rt\?url=/, (route) => route.fulfill({ contentType: "application/octet-stream", body }));
  await page.route("http://feeds.test/**", (route) =>
    route.fulfill({ contentType: "application/octet-stream", headers: { "Access-Control-Allow-Origin": "*" }, body }),
  );
  await page.clock.setFixedTime(new Date("2026-10-23T07:55:00-04:00"));
}

async function pick(page, field, text) {
  const input = page.getByLabel(field, { exact: true });
  await input.fill(text);
  await expect(page.getByRole("option").first()).toBeVisible();
  await input.press("Enter");
}

async function runUnderPolicy(page, prefix) {
  await feeds(page);
  const violations = await watchPolicy(page);

  const ride = await page.goto(`${prefix}/ride-now.html`);
  expect(ride.headers()["content-security-policy"], "ride-now.html carries its policy").toContain("script-src 'self' 'wasm-unsafe-eval'");
  await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toContainText("Main Station", { timeout: 10_000 });
  await expect(page.getByTestId("live-feeds")).toContainText("Live");
  await expect(page.getByTestId("ride-map")).toBeVisible();
  await pick(page, "stop name", "main 1s");
  await expect(page.getByTestId("board-rows").locator("li").first()).toBeVisible();
  await page.getByRole("button", { name: "Directions from here" }).click();
  await pick(page, "To", "main 2");
  await expect(page.getByTestId("itinerary")).toBeVisible({ timeout: 10_000 });
  expect(await violations(), "ride now").toEqual([]);

  const display = await page.goto(`${prefix}/stop-display.html?stop=A&layout=routes`);
  expect(display.headers()["content-security-policy"], "stop-display.html carries its policy").toContain("'wasm-unsafe-eval'");
  expect(display.headers()["permissions-policy"]).toContain("geolocation=()");
  await expect(page.getByTestId("display-group").first()).toBeVisible({ timeout: 10_000 });
  await expect(page.getByTestId("display-live")).toContainText("Live");
  expect(await violations(), "stop display").toEqual([]);
}

test("the pages run under the server's policy", async ({ page }) => {
  await runUnderPolicy(page, "");
});

test("a policy violation is reported, so an empty list means none", async ({ page }) => {
  const violations = await watchPolicy(page);
  await page.goto("/stop-display.html?stop=A&layout=routes");
  await page.evaluate(() => {
    const s = document.createElement("script");
    s.textContent = "window.__ran = true";
    document.head.append(s);
  });
  await expect.poll(violations).toContainEqual(expect.stringContaining("script-src"));
  expect(await page.evaluate(() => window.__ran)).toBeUndefined();
});

test.describe("static export", () => {
  test.use({ baseURL: "http://127.0.0.1:8087" });

  test("each page runs under its bundle's _headers", async ({ page }) => {
    await feeds(page);
    const violations = await watchPolicy(page);

    const ride = await page.goto("/ride/ride-now.html");
    expect(ride.headers()["content-security-policy"], "ride-now.html carries its policy").toContain("script-src 'self' 'wasm-unsafe-eval'");
    expect(ride.headers()["permissions-policy"]).toContain("geolocation=(self)");
    expect(ride.headers()["x-content-type-options"]).toBe("nosniff");
    await expect(page.getByTestId("nearby").getByTestId("stop-card").first()).toContainText("Main Station", { timeout: 10_000 });
    await expect(page.getByTestId("live-feeds")).toContainText("Live");
    await expect(page.getByTestId("ride-map")).toBeVisible();
    await pick(page, "stop name", "main 1s");
    await expect(page.getByTestId("board-rows").locator("li").first()).toBeVisible();
    expect(await violations(), "static ride now").toEqual([]);

    const display = await page.goto("/display/stop-display.html?stop=A&layout=routes");
    expect(display.headers()["content-security-policy"], "stop-display.html carries its policy").toContain("'wasm-unsafe-eval'");
    expect(display.headers()["content-security-policy"]).not.toContain("openfreemap");
    await expect(page.getByTestId("display-group").first()).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId("display-live")).toContainText("Live");
    expect(await violations(), "static stop display").toEqual([]);
  });
});
