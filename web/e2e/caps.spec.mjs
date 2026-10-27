// Capability-bus PoC e2e: the assembly, the composition, and the
// degraded state — the three things the horizontal-capability thesis
// promises, exercised in the browser against the hash-pinned
// manifest the build generated.
//
//   1. The manifest loads; both capabilities assemble (ready).
//   2. The composition runs: the same feed bytes into two modules,
//      hashes asserted equal at the host, departures via gtfs,
//      a plan via router.
//   3. Degraded assembly: with the router module unfetchable, the
//      resolver marks it unavailable and the gtfs half still serves —
//      the app degrades per capability, never wholesale.

import { test, expect } from "@playwright/test";

async function openPanel(page) {
  await page.goto("/");
  // The app module awaits wasm instantiation before it installs its
  // e2e hooks — wait for the hook, then pin the clock.
  await page.waitForFunction(() => typeof window.__setNow === "function");
  await page.evaluate(() => window.__setNow({ date: "2026-10-23", secs: 27000 }));
  // The DataSources modal is the app config surface; the capability
  // panel is its bottom section.
  await page.locator("#data-sources").evaluate((el) => el.classList.remove("hidden"));
}

test("the manifest assembles both capabilities", async ({ page }) => {
  await openPanel(page);
  const panel = page.getByTestId("caps-panel");
  await expect(panel).toBeVisible();
  await expect(panel.getByTestId("cap-gtfs")).toContainText("ready", { timeout: 10_000 });
  await expect(panel.getByTestId("cap-router")).toContainText("ready");
  // Sizes render from the manifest — the size budget is visible.
  await expect(panel.getByTestId("cap-gtfs")).toContainText("KiB");
  // The first capability extracted from the monolith assembles too:
  // three capabilities, one resolver.
  await expect(panel.getByTestId("cap-analysis")).toContainText("ready");
  await expect(panel).toContainText("analysis");
  // And the second (the codec): bootstrap and publish through the bus.
  await expect(panel.getByTestId("cap-network")).toContainText("ready");
  await expect(panel).toContainText("network");
  // And the third (the realtime decoder): the dispatch leg assembles.
  await expect(panel.getByTestId("cap-rt")).toContainText("ready");
  await expect(panel).toContainText("rt");
  // And the street graph (U1): the detour editor's snap/route reads
  // the bus now — the monolith street ops are gone.
  await expect(panel.getByTestId("cap-streets")).toContainText("ready");
  await expect(panel).toContainText("streets");
});

test("the composition runs across two modules", async ({ page }) => {
  await openPanel(page);
  await page.getByTestId("caps-run").click();
  await expect(page.getByTestId("caps-result")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("caps-result")).toContainText("Composition ok");
  // The composition moment: hash handoff asserted, not assumed.
  await expect(page.getByTestId("caps-panel")).toContainText("hash handoff");
  // Departures through gtfs, the plan through router — 08:10 arrival.
  await expect(page.getByTestId("caps-panel")).toContainText("departures");
  await expect(page.getByTestId("caps-panel")).toContainText("08:10");
});

test("a missing capability degrades the assembly, not the app", async ({ page }) => {
  await openPanel(page);
  await page.getByTestId("caps-degrade").click();
  // gtfs still assembles and serves; router is unavailable.
  await expect(page.getByTestId("caps-result")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("caps-result")).toContainText("capability unavailable");
  await expect(page.getByTestId("caps-panel")).toContainText("unavailable");
  // The gtfs half ran: departures served before planning was skipped.
  await expect(page.getByTestId("caps-panel")).toContainText("departures");
  // And the failed fetch did not poison the resolver: the error is
  // recorded, the status speaks.
  await expect(page.getByTestId("cap-router")).toContainText("unavailable");
});

test("documents migrate forward and roll back without loss", async ({ page }) => {
  await openPanel(page);

  // Seed a v1 watchlist: the board answers with v1 notes as labels.
  await page.getByTestId("caps-seed").click();
  await expect(page.getByTestId("caps-doc-version")).toContainText("v1", { timeout: 10_000 });
  await expect(page.getByTestId("caps-board")).toContainText("home");
  await expect(page.getByTestId("caps-board")).toContainText("work");

  // Migrate forward: v2, the dropped color REPORTED, labels preserved.
  await page.getByTestId("caps-migrate").click();
  await expect(page.getByTestId("caps-doc-version")).toContainText("v2");
  await expect(page.getByTestId("caps-panel")).toContainText("dropped_field");
  await expect(page.getByTestId("caps-board")).toContainText("home");
  // The history chain shows both versions.
  await expect(page.getByTestId("caps-panel")).toContainText("v2 ← v1");

  // Roll back: the old version still queries — nothing was destroyed.
  await page.getByTestId("caps-rollback").click();
  await expect(page.getByTestId("caps-doc-version")).toContainText("v1");
  await expect(page.getByTestId("caps-board")).toContainText("home");
  // Rolling back twice runs out of history: an honest message, not a
  // crash.
  await page.getByTestId("caps-rollback").click();
  await expect(page.getByTestId("caps-doc-message")).toContainText("nothing to roll back");
});
