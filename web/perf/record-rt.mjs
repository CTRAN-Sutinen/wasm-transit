// Record the agency's live feeds as a replayable sequence for the perf
// runner (docs/TESTING.md, "Performance baselines"): every INTERVAL seconds, fetch each feed
// web/public/live-feeds.json names and write <dir>/<n>-<kind>.pb.
//
//   node web/perf/record-rt.mjs [count=20] [dir=testdata/perf-rt] [interval=30]

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const [count = "20", dir = "testdata/perf-rt", interval = "30"] = process.argv.slice(2);
const out = path.resolve(root, dir);
const feeds = JSON.parse(fs.readFileSync(path.join(root, "web/public/live-feeds.json"), "utf8"));
const kinds = Object.entries(feeds).filter(([, url]) => url);

fs.mkdirSync(out, { recursive: true });
for (let n = 0; n < Number(count); n++) {
  const started = Date.now();
  for (const [kind, url] of kinds) {
    const resp = await fetch(url);
    if (!resp.ok) throw new Error(`${kind}: HTTP ${resp.status}`);
    const bytes = Buffer.from(await resp.arrayBuffer());
    fs.writeFileSync(path.join(out, `${String(n).padStart(3, "0")}-${kind}.pb`), bytes);
  }
  console.log(`${n + 1}/${count} recorded`);
  if (n + 1 < Number(count)) await new Promise((r) => setTimeout(r, Number(interval) * 1000 - (Date.now() - started)));
}
