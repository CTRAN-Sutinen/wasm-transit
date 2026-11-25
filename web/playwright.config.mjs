import { defineConfig } from "@playwright/test";
import os from "node:os";
import path from "node:path";

const buildsDist = path.join(os.tmpdir(), "transit-e2e-builds");
const releasesDist = path.join(os.tmpdir(), "transit-e2e-releases");
// D-131: the spec uploads a third export, the stop display over the
// overnight feed, over /display/ as a redeploy.
const staticRoot = path.join(os.tmpdir(), "transit-e2e-static");
const staticNext = path.join(os.tmpdir(), "transit-e2e-static-next");
const staticFetchRoot = path.join(os.tmpdir(), "transit-e2e-static-fetch");
// `src-<id>` is the copy of a UI's bundle that the server commits.
// `host-<id>` is what the UI's host serves.
const uiOriginsRoot = path.join(os.tmpdir(), "transit-e2e-ui-origins");
const offlineDist = path.join(os.tmpdir(), "transit-e2e-offline");

// Each server has its own port, never reused, because a shared server
// carries a live edit document and the suite must be hermetic. The :8084
// server pins the date (-today, before the spec's picks and inside the
// fixture's calendar) so a spec can publish picks to it.
export default defineConfig({
  testDir: "./e2e",
  timeout: 20_000,
  retries: 0,
  use: {
    baseURL: "http://127.0.0.1:8081",
    // The rider pages register a service worker (web/service-worker.js),
    // and page.route never sees what a worker answers. So every spec that
    // serves its own feeds and edits runs without one.
    // e2e/deploy-builds.spec.mjs, e2e/deploy-rollback.spec.mjs, and
    // e2e/offline-staff.spec.mjs allow it, each on its own server.
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
        'bash -c \'exec go run . -addr 127.0.0.1:8084 -today 2026-08-01 -store mem:// -feed ../web/e2e/fixtures/sample-gtfs.zip -work-orders stub:Maintenance -attachments mem://\'',
      cwd: "../server",
      url: "http://127.0.0.1:8084",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      command: "node e2e/fake-idp.mjs 8089 transit-server",
      url: "http://127.0.0.1:8089/.well-known/openid-configuration",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      command:
        "bash -c 'exec go run . -addr 127.0.0.1:8090 -today 2026-08-01 -store mem:// -feed ../web/e2e/fixtures/sample-gtfs.zip -oidc-issuer http://127.0.0.1:8089 -oidc-jwks-url http://127.0.0.1:8089/jwks -oidc-audience transit-server -oidc-client-id transit-pages -oidc-scope \"openid profile offline_access transit.write\" -policy ../web/e2e/fixtures/signin-policy.json -attachments mem://'",
      cwd: "../server",
      url: "http://127.0.0.1:8090",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      command: `bash -c 'rm -rf "${buildsDist}" && cp -r ../web/dist "${buildsDist}" && exec go run . -addr 127.0.0.1:8085 -dist "${buildsDist}" -feed ../web/e2e/fixtures/sample-gtfs.zip'`,
      cwd: "../server",
      url: "http://127.0.0.1:8085",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      command: `bash -c 'rm -rf "${releasesDist}" && cp -r ../web/dist "${releasesDist}" && exec go run . -addr 127.0.0.1:8086 -store mem:// -dist "${releasesDist}" -feed ../web/e2e/fixtures/sample-gtfs.zip'`,
      cwd: "../server",
      url: "http://127.0.0.1:8086",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      command: `bash -c 'rm -rf "${offlineDist}" && cp -r ../web/dist "${offlineDist}" && exec go run . -addr 127.0.0.1:8095 -today 2026-08-01 -store mem:// -dist "${offlineDist}" -feed ../web/e2e/fixtures/sample-gtfs.zip -work-orders stub:Maintenance -attachments mem://'`,
      cwd: "../server",
      url: "http://127.0.0.1:8095/healthz",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      // No Go server serves this one, because the export writes the bundle and
      // exits.
      command: `bash -c 'rm -rf "${staticRoot}" "${staticNext}" && for ui in ride-now:ride stop-display:display; do go run . -export "${staticRoot}/\${ui#*:}" -dist "../web/dist-ui/\${ui%%:*}" -feed ../web/e2e/fixtures/sample-gtfs.zip -streets ../web/e2e/fixtures/walk-streets.geojson -live-feeds ../web/e2e/fixtures/static-live-feeds.json || exit 1; done && go run . -export "${staticNext}" -dist ../web/dist-ui/stop-display -feed ../web/e2e/fixtures/overnight-gtfs.zip && exec node ../web/e2e/static-server.mjs "${staticRoot}" 8087'`,
      cwd: "../server",
      url: "http://127.0.0.1:8087/display/manifest",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      command: `bash -c 'rm -rf "${staticFetchRoot}" && for ui in ride-now:ride stop-display:display; do go run . -export "${staticFetchRoot}/\${ui#*:}" -dist "../web/dist-ui/\${ui%%:*}" -feed-url http://localhost:8088/feeds/feed.zip -live-feeds ../web/e2e/fixtures/static-live-feeds.json || exit 1; done && mkdir -p "${staticFetchRoot}/feeds" && cp ../web/e2e/fixtures/sample-gtfs.zip "${staticFetchRoot}/feeds/feed.zip" && exec node ../web/e2e/static-server.mjs "${staticFetchRoot}" 8088 cors'`,
      cwd: "../server",
      url: "http://127.0.0.1:8088/display/manifest",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      command: `bash -c 'rm -rf "${uiOriginsRoot}" && mkdir -p "${uiOriginsRoot}" && cp -r ../web/dist-ui/stops "${uiOriginsRoot}/src-stops" && cp -r ../web/dist-ui/service-change "${uiOriginsRoot}/src-service-change" && exec go run . -addr 127.0.0.1:8092 -today 2026-08-01 -store mem:// -feed ../web/e2e/fixtures/sample-gtfs.zip -ui-dist "stops=${uiOriginsRoot}/src-stops,service-change=${uiOriginsRoot}/src-service-change" -oidc-issuer http://127.0.0.1:8089 -oidc-jwks-url http://127.0.0.1:8089/jwks -oidc-audience transit-server -oidc-client-id transit-pages -oidc-scope "openid profile offline_access transit.write" -policy ../web/e2e/fixtures/ui-origins-policy.json -attachments mem://'`,
      cwd: "../server",
      url: "http://127.0.0.1:8092/healthz",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
    ...[
      ["stops", 8093],
      ["service-change", 8094],
    ].map(([ui, port]) => ({
      command: `bash -c 'node ../tools/ui-deploy.mjs http://127.0.0.1:8092 ${ui} "${uiOriginsRoot}/host-${ui}" && exec node e2e/static-server.mjs "${uiOriginsRoot}/host-${ui}" ${port} forward=http://127.0.0.1:8092'`,
      url: `http://127.0.0.1:${port}/${ui}.html`,
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    })),
  ],
});
