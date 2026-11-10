import { defineConfig } from "@playwright/test";
import os from "node:os";
import path from "node:path";

/// The copy of the build the :8085 server serves, which
/// e2e/deploy-builds.spec.mjs publishes a new build over (the same
/// path there).
const buildsDist = path.join(os.tmpdir(), "transit-e2e-builds");
/// The copy of the build the :8086 server commits to its store, which
/// e2e/deploy-rollback.spec.mjs publishes a new release over.
const releasesDist = path.join(os.tmpdir(), "transit-e2e-releases");
/// The static host's root (e2e/static-deploy.spec.mjs, D-131): the
/// server's export of each rider UI alone (web/dist-ui/<id>, `just
/// build-ui`) over the fixture feed — ride-now under /ride/, the stop
/// display under /display/ — served by a plain static file server; and
/// a third export (the stop display over the overnight feed) the spec
/// uploads over /display/ as a redeploy.
const staticRoot = path.join(os.tmpdir(), "transit-e2e-static");
const staticNext = path.join(os.tmpdir(), "transit-e2e-static-next");
/// A serverless deployment that follows the agency's GTFS URL
/// (e2e/static-feed-url.spec.mjs, D-132): each rider UI's bundle (under
/// /ride/ and /display/) and the feed under /feeds/, served with CORS.
const staticFetchRoot = path.join(os.tmpdir(), "transit-e2e-static-fetch");

// E2E tests run against the built app (npm run build) served by the
// Go sync server — on their OWN port, never reused: a shared server
// carries a live edit document, and the suite must be hermetic (nor
// should test records land in a dev server someone is using). The
// second server publishes the fixture feed's network and the walking
// fixture's streets and walk table (e2e/published-network.spec.mjs);
// the first publishes none. The third runs the sync store (in memory,
// fresh per run: -store mem://) on a pinned date (-today: before the spec's picks,
// inside the fixture's calendar) so a spec can publish picks to it
// (e2e/published-picks.spec.mjs). In store mode a server commits its
// -dist as a build and serves the pages from it (docs/architecture/release-artifacts.md); the fifth rolls one back. The sixth is no Go
// server at all: the server's static export, on a plain file server.
export default defineConfig({
  testDir: "./e2e",
  timeout: 20_000,
  retries: 0,
  use: {
    baseURL: "http://127.0.0.1:8081",
    // The rider pages register a service worker (web/service-worker.js),
    // and page.route never sees what a worker answers — every spec that
    // serves its own feeds and edits runs without one.
    // e2e/deploy-builds.spec.mjs and e2e/deploy-rollback.spec.mjs
    // allow it, each on its own server.
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
    {
      // Releases and rollback (e2e/deploy-rollback.spec.mjs): the store
      // mode server commits its copy of the build to the build pointer
      // and serves the pages from it.
      command: `bash -c 'rm -rf "${releasesDist}" && cp -r ../web/dist "${releasesDist}" && exec go run . -addr 127.0.0.1:8086 -store mem:// -dist "${releasesDist}" -feed ../web/e2e/fixtures/sample-gtfs.zip'`,
      cwd: "../server",
      url: "http://127.0.0.1:8086",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      // A static deployment (e2e/static-deploy.spec.mjs, D-131): no Go
      // server serves it — the export writes the bundle and exits.
      command: `bash -c 'rm -rf "${staticRoot}" "${staticNext}" && for ui in ride-now:ride stop-display:display; do go run . -export "${staticRoot}/\${ui#*:}" -dist "../web/dist-ui/\${ui%%:*}" -feed ../web/e2e/fixtures/sample-gtfs.zip -streets ../web/e2e/fixtures/walk-streets.geojson -live-feeds ../web/e2e/fixtures/static-live-feeds.json || exit 1; done && go run . -export "${staticNext}" -dist ../web/dist-ui/stop-display -feed ../web/e2e/fixtures/overnight-gtfs.zip && exec node ../web/e2e/static-server.mjs "${staticRoot}" 8087'`,
      cwd: "../server",
      url: "http://127.0.0.1:8087/display/manifest",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      // The serverless deployment that fetches its GTFS (D-132): the
      // export names the feed's URL and imports nothing.
      command: `bash -c 'rm -rf "${staticFetchRoot}" && for ui in ride-now:ride stop-display:display; do go run . -export "${staticFetchRoot}/\${ui#*:}" -dist "../web/dist-ui/\${ui%%:*}" -feed-url http://localhost:8088/feeds/feed.zip -live-feeds ../web/e2e/fixtures/static-live-feeds.json || exit 1; done && mkdir -p "${staticFetchRoot}/feeds" && cp ../web/e2e/fixtures/sample-gtfs.zip "${staticFetchRoot}/feeds/feed.zip" && exec node ../web/e2e/static-server.mjs "${staticFetchRoot}" 8088 cors'`,
      cwd: "../server",
      url: "http://127.0.0.1:8088/display/manifest",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
  ],
});
