import { defineConfig } from "@playwright/test";

// The perf runner (`just perf`, docs/TESTING.md "Performance baselines").
// The Go server serves the profiling build (web/dist-perf: unminified, with
// name sections in the capability modules) with the real feed. It reports
// and never fails on a number.
const feed = process.env.PERF_FEED ?? "../testdata/ctran-gtfs.zip";

export default defineConfig({
  testDir: ".",
  testMatch: "*.perf.mjs",
  outputDir: "../test-results/perf-artifacts",
  timeout: 60 * 60_000,
  retries: 0,
  workers: 1,
  use: {
    baseURL: "http://127.0.0.1:8082",
    viewport: { width: 1280, height: 800 },
    timezoneId: "America/Los_Angeles",
  },
  webServer: {
    command: `go run . -addr 127.0.0.1:8082 -dist ../web/dist-perf -feed ${feed} -publisher C-TRAN`,
    cwd: "../../server",
    url: "http://127.0.0.1:8082",
    reuseExistingServer: false,
    timeout: 120_000,
    stdout: "ignore",
    stderr: "pipe",
  },
});
