#!/usr/bin/env node
// Route 99 built through the agent runner — the stand-in for step 2.3
// while the service-change UI cannot create a route (QAF-112). Delete
// this file in the change that fixes QAF-112.
//
// The C-TRAN feed imported, route 99's edits applied (cap-author
// `apply`: its stops, route, two patterns, and trips), validated, and
// exported as a GTFS zip the service-change page imports as a draft:
//
//   node qa/route-99/build-route.mjs   → .qa/route-99/ctran-route99.zip
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { STOPS, LENGTH_M, RUN_SECS, ROUTE, SERVICE } from "./route.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const out = path.join(root, ".qa/route-99");
fs.mkdirSync(out, { recursive: true });

const edits = [];
for (const [id, name, lat, lon] of STOPS) edits.push({ edit: "upsert_stop", id, stop: { name, lat, lon, info: { code: id }, transfer: false } });
edits.push({
  edit: "upsert_route",
  id: ROUTE.id,
  route: {
    name: ROUTE.id, mode: "bus", long_name: ROUTE.long_name, color: "3777bc", text_color: "ffffff", sort_order: 26,
    operator: "C-TRAN", fare_network: "LOCAL",
    directions: { 0: { name: "East Fourth Plain" }, 1: { name: "Washougal" } },
  },
});
const ways = [
  ["0", STOPS, "East Fourth Plain TC", 0],
  ["1", [...STOPS].reverse().map(([id, n, la, lo, d, t]) => [id, n, la, lo, LENGTH_M - d, RUN_SECS - t]), "Washougal TC", 900],
];
for (const [dir, seq, headsign, offset] of ways) {
  const pattern = `pat-${ROUTE.id}-${dir}`;
  edits.push({
    edit: "upsert_pattern",
    id: pattern,
    pattern: {
      route: ROUTE.id, direction: dir, name: `${ROUTE.id} → ${headsign}`,
      stops: seq.map(([stop, , , , d]) => ({ stop, timepoint: true, distance_m: d })),
      timing: [{ start_secs: 0, end_secs: 30 * 3600, at_timepoints: seq.map((s) => s[5]) }],
    },
  });
  for (const [type, first, last, every] of SERVICE) {
    const trips = [];
    for (let t = first * 3600 + offset; t <= last * 3600; t += every * 60)
      trips.push({
        id: `${ROUTE.id}-${dir}-${type}-${t / 60}`,
        departs: { kind: "at", start_secs: t },
        times: seq.map((s) => ({ arrival_secs: s[5], departure_secs: s[5] })),
        headsign,
      });
    edits.push({ edit: "upsert_pattern_trips", id: `${type}:${pattern}`, pattern_trips: { pattern, service_type: type, trips } });
  }
}
const applyFile = path.join(out, "apply.json");
fs.writeFileSync(applyFile, JSON.stringify({ op: "apply", network_hash: "$imp.network_hash", edits }));

const lines = execFileSync(
  "node",
  [
    "tools/caps-run.mjs", "network",
    "--load", "feed:testdata/ctran-gtfs.zip",
    "--as", "imp", "--op", '{"op":"import","feed_hash":"$load.feed_hash"}',
    "--cap", "author", "--op", '{"op":"load_network","network":"$imp.network"}',
    "--as", "app", "--op", `@${applyFile}`,
    "--cap", "network", "--op", '{"op":"load_network","network":"$app.network"}',
    "--op", '{"op":"validate","network_hash":"$app.network_hash"}',
    "--op", '{"op":"export","network_hash":"$app.network_hash","boundary_hash":"$imp.boundary_hash"}',
  ],
  { cwd: root, maxBuffer: 1 << 30, timeout: 300_000 },
).toString().trim().split("\n").map((l) => JSON.parse(l));

const applied = lines.find((l) => l.as === "app").ok;
const exported = lines.at(-1).ok;
if (!applied || applied.issues.length || !exported?.zip_b64) {
  console.error("route 99 was refused:", JSON.stringify(applied?.issues ?? lines.at(-1)).slice(0, 2000));
  process.exit(1);
}
const zip = path.join(out, "ctran-route99.zip");
fs.writeFileSync(zip, Buffer.from(exported.zip_b64, "base64"));
console.log(`network ${applied.network_hash} (${applied.warnings.length} warnings: trips on no block)`);
console.log(`wrote ${zip} (${exported.summary.routes} routes, ${exported.summary.stops} stops, ${exported.summary.trips} trips)`);
