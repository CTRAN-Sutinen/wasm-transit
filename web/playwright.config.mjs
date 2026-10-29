import { defineConfig } from "@playwright/test";

// E2E tests run against the built app (npm run build) served by the
// Go sync server — on their OWN port, never reused: a shared server
// carries a live edit document, and the suite must be hermetic (nor
// should test records land in a dev server someone is using). The
// second server publishes the fixture feed's network
// (e2e/published-network.spec.mjs); the first publishes none.
export default defineConfig({
  testDir: "./e2e",
  timeout: 20_000,
  retries: 0,
  use: {
    baseURL: "http://127.0.0.1:8081",
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
      command: "go run . -addr 127.0.0.1:8083 -feed ../web/e2e/fixtures/sample-gtfs.zip",
      cwd: "../server",
      url: "http://127.0.0.1:8083",
      reuseExistingServer: false,
      stdout: "ignore",
      stderr: "pipe",
    },
  ],
});
