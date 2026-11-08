import { defineConfig } from "@playwright/test";
import os from "node:os";
import path from "node:path";

/// The copy of the build the :8085 server serves, which
/// e2e/deploy-builds.spec.mjs publishes a new build over (the same
/// path there).
const buildsDist = path.join(os.tmpdir(), "transit-e2e-builds");

// E2E tests run against the built app (npm run build) served by the
// Go sync server — on their OWN port, never reused: a shared server
// carries a live edit document, and the suite must be hermetic (nor
// should test records land in a dev server someone is using). The
// second server publishes the fixture feed's network and the walking
// fixture's streets and walk table (e2e/published-network.spec.mjs);
// the first publishes none. The third runs the sync store (in memory,
// fresh per run: -store mem://) on a pinned date (-today: before the spec's picks,
// inside the fixture's calendar) so a spec can publish picks to it
// (e2e/published-picks.spec.mjs).
export default defineConfig({
  testDir: "./e2e",
  timeout: 20_000,
  retries: 0,
  use: {
    baseURL: "http://127.0.0.1:8081",
    // The rider pages register a service worker (web/service-worker.js),
    // and page.route never sees what a worker answers — every spec that
    // serves its own feeds and edits runs without one.
    // e2e/deploy-builds.spec.mjs allows it, on its own server.
    serviceWorkers: "block",
  },
  webServer: [
    {
      command: "go run . -addr 127.0.0.1:8081",
      cwd: "../server",
      url: "http://127.0.0.1:8081",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      command:
        "go run . -addr 127.0.0.1:8083 -feed ../web/e2e/fixtures/sample-gtfs.zip -streets ../web/e2e/fixtures/walk-streets.geojson",
      cwd: "../server",
      url: "http://127.0.0.1:8083",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      command:
        'bash -c \'exec go run . -addr 127.0.0.1:8084 -today 2026-08-01 -store mem:// -feed ../web/e2e/fixtures/sample-gtfs.zip\'',
      cwd: "../server",
      url: "http://127.0.0.1:8084",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      // The build lifecycle and offline (e2e/deploy-builds.spec.mjs):
      // a copy of the build, so the spec can publish a new one.
      command: `bash -c 'rm -rf "${buildsDist}" && cp -r ../web/dist "${buildsDist}" && exec go run . -addr 127.0.0.1:8085 -dist "${buildsDist}" -feed ../web/e2e/fixtures/sample-gtfs.zip'`,
      cwd: "../server",
      url: "http://127.0.0.1:8085",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
  ],
});
