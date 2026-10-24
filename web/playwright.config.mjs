import { defineConfig } from "@playwright/test";

// E2E tests run against the built app (npm run build) served by the
// Go sync server on :8080. The server is started/stopped by the
// runner unless one is already running (reuseExistingServer).
export default defineConfig({
  testDir: "./e2e",
  timeout: 20_000,
  retries: 0,
  use: {
    baseURL: "http://127.0.0.1:8080",
  },
  webServer: {
    command: "go run .",
    cwd: "../server",
    url: "http://127.0.0.1:8080",
    reuseExistingServer: true,
    stdout: "ignore",
    stderr: "pipe",
  },
});
