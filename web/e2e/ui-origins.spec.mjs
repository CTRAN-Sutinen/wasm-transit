// Each UI at its own URL (D-171; docs/architecture/release-artifacts.md, "Each
// UI at its own URL"). One store-mode server with sign-in on keeps each UI's
// bundle as its own build. Each host (e2e/static-server.mjs, forward=) serves
// the build that tools/ui-deploy.mjs wrote. One UI rolls back alone. The ports
// are :8092 for the server, :8093 for the stops host, and :8094 for the
// service-change host.

import { test, expect } from "@playwright/test";
import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const server = "http://127.0.0.1:8092";
const idp = "http://127.0.0.1:8089";
const root = path.join(os.tmpdir(), "transit-e2e-ui-origins");
const hosts = { stops: "http://127.0.0.1:8093", "service-change": "http://127.0.0.1:8094" };
const deployTool = path.join(import.meta.dirname, "../../tools/ui-deploy.mjs");

async function head(request, ui) {
  return (await (await request.get(`${server}/pointers/build.${ui}`)).json()).hash;
}

function deploy(ui, build) {
  const next = path.join(root, `next-${ui}`);
  fs.rmSync(next, { recursive: true, force: true });
  execFileSync("node", [deployTool, server, ui, next, ...(build ? [build] : [])], { stdio: "pipe" });
  const served = path.join(root, `host-${ui}`);
  fs.rmSync(served, { recursive: true, force: true });
  fs.renameSync(next, served);
}

async function token(request) {
  const verifier = crypto.randomBytes(32).toString("base64url");
  const challenge = crypto.createHash("sha256").update(verifier).digest("base64url");
  const authorize = new URL(`${idp}/authorize`);
  for (const [k, v] of Object.entries({ client_id: "release", redirect_uri: "http://127.0.0.1/back", code_challenge: challenge, code_challenge_method: "S256", state: "s" }))
    authorize.searchParams.set(k, v);
  const back = await request.get(authorize.toString(), { maxRedirects: 0 });
  const code = new URL(back.headers().location).searchParams.get("code");
  const resp = await request.post(`${idp}/token`, {
    form: { grant_type: "authorization_code", code, code_verifier: verifier, client_id: "release", redirect_uri: "http://127.0.0.1/back" },
  });
  return (await resp.json()).access_token;
}

test("two UIs at two origins: each signs in at its own URL, and one rolls back alone", async ({ page, request }) => {
  test.setTimeout(90_000);

  const redirects = [];
  page.on("request", (r) => {
    if (r.url().startsWith(`${idp}/authorize`)) redirects.push(new URL(r.url()).searchParams.get("redirect_uri"));
  });
  await page.clock.setFixedTime(new Date("2026-11-02T09:00:00-05:00"));
  await page.goto(`${hosts.stops}/stops.html#on=2026-11-02&open=A`);
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 15_000 });

  await page.getByTestId("signin-button").click();
  await expect(page.getByTestId("signin-name")).toHaveText("Rosa Ortiz", { timeout: 15_000 });
  expect(page.url().startsWith(`${hosts.stops}/stops.html`)).toBe(true);
  await page.getByTestId("action-survey").click();
  await page.getByTestId("survey-on_street").fill("Main St");
  await page.getByTestId("survey-save").click();
  await expect(page.getByTestId("stop-surveyed")).toContainText("Last surveyed on 2026-11-02 by Rosa Ortiz.", { timeout: 15_000 });

  await page.goto(`${hosts["service-change"]}/service-change.html`);
  await page.getByRole("button", { name: /Proposal/ }).click();
  await page.getByTestId("signin-button").click();
  await expect(page.getByTestId("signin-name")).toHaveText("Rosa Ortiz", { timeout: 15_000 });
  expect(page.url().startsWith(`${hosts["service-change"]}/service-change.html`)).toBe(true);
  expect(redirects).toEqual([`${hosts.stops}/stops.html`, `${hosts["service-change"]}/service-change.html`]);

  const a = { stops: await head(request, "stops"), "service-change": await head(request, "service-change") };
  const swA = {};
  for (const ui of Object.keys(hosts)) {
    swA[ui] = await (await request.get(`${hosts[ui]}/sw.js`)).text();
    fs.appendFileSync(path.join(root, `src-${ui}`, "sw.js"), `\n// ${ui} build B\n`);
  }
  await expect
    .poll(
      async () => {
        // A request makes the server re-read the UIs' bundles.
        await request.get(`${server}/manifest`);
        return (await head(request, "stops")) !== a.stops && (await head(request, "service-change")) !== a["service-change"];
      },
      { timeout: 15_000 },
    )
    .toBe(true);
  for (const ui of Object.keys(hosts)) {
    deploy(ui);
    expect(await (await request.get(`${hosts[ui]}/sw.js`)).text()).toContain(`${ui} build B`);
  }

  const moved = await request.post(`${hosts.stops}/builds?ui=stops`, {
    data: { build: a.stops },
    headers: { Authorization: `Bearer ${await token(request)}` },
  });
  expect(moved.status(), await moved.text()).toBe(200);
  deploy("stops");
  expect(await head(request, "stops")).toBe(a.stops);
  expect(await (await request.get(`${hosts.stops}/sw.js`)).text(), "the stops UI serves A's bytes").toBe(swA.stops);

  expect(await head(request, "service-change")).not.toBe(a["service-change"]);
  expect(await (await request.get(`${hosts["service-change"]}/sw.js`)).text()).toContain("service-change build B");

  await page.goto(`${hosts.stops}/stops.html#on=2026-11-02&open=A`);
  await expect(page.getByTestId("stop-card")).toBeVisible({ timeout: 15_000 });
});
