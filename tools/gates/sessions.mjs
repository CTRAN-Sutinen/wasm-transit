// The runner's gate (tools/caps-run.mjs): each session is a question that
// a host asks, run through the runner's CLI over the fixtures, with the
// answer it must give. Every op names its documents through result
// references ($load, $name), never a hash copied by hand. A failure names
// the session and this script. sessions.test.mjs is the broken fixture.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const LOAD = "@web/public/caps/sample-network-load.json";
const FEED = "feed:web/public/caps/sample-gtfs.zip";
const RT = "web/e2e/fixtures/sample-rt.pb";
const DIVERSION = (status, detour) =>
  `{"kind":"edit_records","schema_version":2,"records":[{"id":"e1","diversion":{"id":"d1","route_id":"10","direction_id":null,"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":["C"],"detour":${detour},"added_secs":300,"start_date":"2026-10-23","end_date":"2026-10-23"},"status":"${status}","rev":1,"updated_at":1}]}`;
const EDITS = DIVERSION("accepted", "null");
const STOPS_DOC = '{"kind":"stops","schema_version":1,"stops":{"6124":[{"id":"e7f43ace17772","on":"2004-09-01","event":"in_service","lat":45.6934,"lon":-122.6512,"name":"NE 99th St & Hwy 99"},{"id":"ead77f93b9e81","on":"2027-03-03","event":"suspended","until":"2027-07-01"}]}}';
const LIVE_CHAIN = [
  "network", "--as", "feed", "--load", FEED,
  "--as", "net", "--op", '{"op":"import","feed_hash":"$feed.feed_hash"}',
  "--cap", "rt", "--as", "rnet", "--op", '{"op":"load_network","network":"$net.network"}',
  "--as", "bnd", "--op", '{"op":"load_boundary","boundary":"$net.boundary"}',
  "--as", "snap", "--load", RT,
  "--as", "live", "--op", '{"op":"live","network_hash":"$rnet.network_hash","boundary_hash":"$bnd.boundary_hash","rt_hash":"$snap.rt_hash"}',
  "--cap", "board", "--as", "bnet", "--op", '{"op":"load_network","network":"$net.network"}',
  "--as", "blive", "--op", '{"op":"load_live","live":"$live.live"}',
];

export class SessionFailed extends Error {}

// Runs the runner. `fails: true` expects a non-zero exit.
export function caps(args, { env = {}, fails = false, root = ROOT } = {}) {
  let stdout = "", stderr = "", code = 0;
  try {
    stdout = execFileSync("node", ["tools/caps-run.mjs", ...args], { cwd: root, env: { ...process.env, ...env }, encoding: "utf8", maxBuffer: 1 << 28, stdio: ["ignore", "pipe", "pipe"] });
  } catch (e) {
    ({ stdout = "", stderr = "", status: code } = e);
  }
  if (code !== 0 && !fails) throw new SessionFailed(`caps-run exited ${code}: ${stderr.trim().split("\n").at(-1)}`);
  if (code === 0 && fails) throw new SessionFailed("caps-run succeeded where it must fail");
  const lines = stdout.trim() === "" ? [] : stdout.trim().split("\n").map((l) => { try { return JSON.parse(l); } catch { return l; } });
  const named = (name) => {
    const line = lines.find((x) => x?.as === name);
    if (!line?.ok) throw new SessionFailed(`no answer named ${name}${line?.error ? `: ${JSON.stringify(line.error)}` : ""}`);
    return line.ok;
  };
  return { stdout, stderr, lines, named, last: lines.at(-1) };
}

export function has(text, needle) {
  if (!text.includes(needle)) throw new SessionFailed(`the answer lacks ${needle}`);
}

export function must(cond, what) {
  if (!cond) throw new SessionFailed(what);
}

const DAY_STATS = '{"op":"network_stats","network_hash":"$load_network.network_hash","date":"$TODAY","include_routes":false,"include_service_types":false,"include_stops":false}';

async function serviceDate(zone) {
  const { serviceClock } = await import(join(ROOT, "web/src/clock.ts"));
  return serviceClock(Date.now(), zone).date;
}

export const SESSIONS = [
  ["the runner knows which tree its registry was built from (QAF-77)", () => {
    const { stderr } = caps(["list"]);
    must(stderr === "", `caps-run list warns: ${stderr.trim()}`);
  }],
  ["directory: stops, names, nearest (a bay answers as its station), routes, and route 10's patterns", () => {
    has(caps(["directory", "--op", LOAD,
      "--op", '{"op":"stops","network_hash":"$load_network.network_hash","limit":2}',
      "--op", '{"op":"stops_by_ids","network_hash":"$load_network.network_hash","ids":["A","ghost","STN"]}',
      "--op", '{"op":"stops_near","network_hash":"$load_network.network_hash","lat":45.515,"lon":-73.5155,"limit":3}',
      "--op", '{"op":"routes","network_hash":"$load_network.network_hash"}',
      "--op", '{"op":"route_stop_sequence","network_hash":"$load_network.network_hash","route_id":"10"}',
    ]).stdout, '"stop_id":"A"');
  }],
  ["an oracle's answer chains into the ops (--ask)", () => {
    has(caps(["directory", "--op", LOAD,
      "--ask", 'data:application/json,{"features":[{"geometry":{"coordinates":[-73.5155,45.515]},"properties":{"name":"200 Station Rd"}}]}',
      "--op", '{"op":"stops_near","network_hash":"$load_network.network_hash","lat":"$ask.features.0.geometry.coordinates.1","lon":"$ask.features.0.geometry.coordinates.0","limit":1}',
    ]).stdout, '"stop_id":"STN"');
  }],
  ["board: the station pin answers its bays, and the personal board chains its watchlist", () => {
    has(caps(["board", "--op", LOAD,
      "--op", '{"op":"stop_board","network_hash":"$load_network.network_hash","stop_id":"STN","date":"2026-10-23","from_secs":25200,"limit":10}',
      "--op", '{"op":"load_watchlist","document":{"kind":"watchlist","schema_version":2,"stops":[{"id":"A","label":"home"}]}}',
      "--op", '{"op":"watchlist_board","watchlist_hash":"$load_watchlist.watchlist_hash","network_hash":"$load_network.network_hash","date":"2026-10-23","from_secs":25200,"limit":3}',
    ]).stdout, '"label":"home"');
  }],
  ["the registry kernel ops: held names the network, a keep-set drops it, the next ask is unknown_document", () => {
    const dir = mkdtempSync(join(tmpdir(), "caps-gate-"));
    try {
      writeFileSync(join(dir, "keep.json"), '{"keep":[]}');
      const { stdout } = caps(["board", "--op", LOAD, "--load", "258:/dev/null", "--load", `257:${join(dir, "keep.json")}`,
        "--op", '{"op":"stop_board","network_hash":"$load_network.network_hash","stop_id":"A","date":"2026-10-23","from_secs":0,"limit":1}']);
      must(/"held":\[\{"kind":"network".*"dropped":\[\{"kind":"network".*"error":\{"message":"unknown network sha256:[0-9a-f]+: load_network first","code":"unknown_document"/.test(stdout.replace(/\n/g, " ")), "held, dropped, then unknown_document");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }],
  ["network: load_feed into import reports the dropped single-stop trip", () => {
    has(caps(["network", "--load", FEED, "--op", '{"op":"import","feed_hash":"$load.feed_hash"}']).stdout, '"kind":"dropped_trips"');
  }],
  ["`tools author` carries a worked example for each edit kind (QAF-52)", () => {
    const apply = JSON.parse(caps(["tools", "author"]).stdout).tools.apply;
    must(apply.kinds?.some((k) => k.kind === "add_trip" && k.example.edit === "add_trip"), "apply has no add_trip example");
  }],
  ["gtfs: the feed's identity", () => {
    has(caps(["gtfs", "--load", FEED, "--op", '{"op":"feed_info","feed_hash":"$load.feed_hash"}']).stdout, '"feed_version":"2026.10.1"');
  }],
  ["rt: t10's predicted departure at A in one op (08:00 + 120 s)", () => {
    has(caps(["network", "--load", FEED,
      "--as", "net", "--op", '{"op":"import","feed_hash":"$load.feed_hash"}',
      "--cap", "rt", "--as", "rnet", "--op", '{"op":"load_network","network":"$net.network"}',
      "--as", "bnd", "--op", '{"op":"load_boundary","boundary":"$net.boundary"}',
      "--as", "snap", "--load", RT,
      "--op", '{"op":"trip_progress","network_hash":"$rnet.network_hash","boundary_hash":"$bnd.boundary_hash","rt_hash":"$snap.rt_hash","trip_id":"t10"}',
    ]).stdout, '"predicted_departure":28920');
  }],
  ["the clock: $TODAY resolves on the host, whatever day the gate runs", () => {
    has(caps(["calendar", "--op", LOAD, "--op", '{"op":"active_services","network_hash":"$load_network.network_hash","date":"$TODAY"}']).stdout, '"op":"active_services"');
  }],
  ["the runner reads the clock in the agency's zone, never the machine's; --tz names another; no zone fails", async () => {
    for (const tz of ["Asia/Tokyo", "Pacific/Honolulu"]) {
      const { last } = caps(["analysis", "--op", LOAD, "--op", DAY_STATS], { env: { TZ: tz } });
      must(last.ok?.stats?.date === await serviceDate("America/Montreal"), `from ${tz}, $TODAY is not the network's date`);
    }
    const { last } = caps(["analysis", "--tz", "Asia/Tokyo", "--op", LOAD, "--op", DAY_STATS]);
    must(last.ok?.stats?.date === await serviceDate("Asia/Tokyo"), "--tz Asia/Tokyo is not Tokyo's date");
    const { stdout, stderr } = caps(["analysis", "--op", '{"op":"network_stats","network_hash":"sha256:00","date":"$TODAY"}'], { fails: true });
    has(stdout + stderr, "--tz");
  }],
  ["discovery stays cheap: `tools --brief`, and one op in full", () => {
    const text = caps(["tools", "analysis", "--brief"]).stdout;
    const j = JSON.parse(text);
    const ops = Object.values(j.tools);
    must(text.length <= 30000 && ops.length >= 10, "the brief catalog is too long or too short");
    must(ops.every((o) => typeof o.summary === "string" && o.summary.length <= 400), "a summary is missing or too long");
    must(j.tools.network_stats.example.request.network_hash !== undefined, "network_stats has no example request");
    const one = JSON.parse(caps(["tools", "analysis", "stop_gaps"]).stdout);
    must(one.op === "stop_gaps" && one.description.length > 100 && one.example?.request?.op === "stop_gaps", "stop_gaps in full");
  }],
  ["calendar: WK's dates read backwards across the removed 10-29", () => {
    has(caps(["calendar", "--op", LOAD, "--op", '{"op":"service_dates","network_hash":"$load_network.network_hash","service_type":"WK","from":"2026-10-23","to":"2026-11-03"}']).stdout,
      '"dates":["2026-10-23","2026-10-26","2026-10-27","2026-10-28","2026-10-30","2026-11-02","2026-11-03"]');
  }],
  ["analysis: stop A's day as gaps (QAF-25)", () => {
    has(caps(["analysis", "--op", LOAD, "--op", '{"op":"stop_gaps","network_hash":"$load_network.network_hash","date":"2026-10-23","stop_id":"A"}']).stdout, '"departures":3,"first_departure_secs":28800');
  }],
  ["analysis: the day's hourly profile (QAF-4)", () => {
    has(caps(["analysis", "--op", LOAD, "--op", '{"op":"hourly_trips","network_hash":"$load_network.network_hash","date":"2026-10-23"}']).stdout, '"hour":8,"trips":5,"service_types":[{"service_type":"WK","trips":5}]');
  }],
  ["analysis: the schedule texture (QAF-16)", () => {
    has(caps(["analysis", "--op", LOAD, "--op", '{"op":"schedule_texture","network_hash":"$load_network.network_hash","members":["WK"]}']).stdout, '"on_clockface":2,"pattern_trips":[');
  }],
  ["analysis: routes 10 and 20 share A to B (QAF-22)", () => {
    has(caps(["analysis", "--op", LOAD, "--op", '{"op":"route_overlap","network_hash":"$load_network.network_hash","date":"2026-10-23"}']).stdout,
      '"a":"10","b":"20","shared_stops":2,"shared_segments":1,"stack_stop":"A","stack_departures":3');
  }],
  ["analysis: the timed transfer at A under a 25-minute window (QAF-27)", () => {
    has(caps(["analysis", "--op", LOAD, "--op", '{"op":"timed_transfers","network_hash":"$load_network.network_hash","date":"2026-10-23","stop_id":"A","window":{"min_secs":60,"max_secs":1500}}']).stdout,
      '"from_route":"10","to_route":"20","arrival_secs":28800,"arrival_stop":"A","departure_secs":30000,"departure_stop":"A","wait_secs":1200');
  }],
  ["one session across two modules: the imported document reaches the board by reference", () => {
    const r = caps(["network", "--load", FEED,
      "--as", "net", "--op", '{"op":"import","feed_hash":"$load.feed_hash"}',
      "--cap", "board", "--as", "board_net", "--op", '{"op":"load_network","network":"$net.network"}',
      "--op", '{"op":"stop_board","network_hash":"$board_net.network_hash","stop_id":"A","date":"$TODAY","from_secs":"$NOW_SECS","limit":1}']);
    must(r.named("net").network_hash === r.named("board_net").network_hash, "the two modules disagree on the network's hash");
    must(r.last.ok?.op === "stop_board", "the board did not answer");
  }],
  ["a session lasts across commands, registers what it holds, lists it, and its journal replays (tools/journal.mjs)", () => {
    const store = mkdtempSync(join(tmpdir(), "caps-sessions-"));
    const env = { CAPS_RUN_SESSIONS: store };
    try {
      const imported = caps(["network", "--session", "gate", "--load", FEED, "--as", "net", "--op", '{"op":"import","feed_hash":"$load.feed_hash"}'], { env }).named("net").network_hash;
      const second = caps(["board", "--session", "gate",
        "--as", "bnet", "--op", '{"op":"load_network","network":"$net.network"}',
        "--op", '{"op":"stop_board","network_hash":"$bnet.network_hash","stop_id":"A","date":"2026-10-23","from_secs":25200,"limit":1}'], { env });
      must(!second.lines.some((x) => x.ok?.op === "import"), "the second command imported again");
      must(second.named("bnet").network_hash === imported && second.last.ok?.op === "stop_board", "the board did not read the session's network");
      const third = caps(["board", "--session", "gate", "--op", '{"op":"stop_board","network_hash":"$net.network_hash","stop_id":"A","date":"2026-10-23","from_secs":25200,"limit":1}'], { env }).last;
      must(third.ok?.op === "stop_board" && third.registered?.includes(`network ${imported}`), "an op by a held hash did not register the document");
      const listed = JSON.parse(caps(["session", "gate"], { env }).stdout).results.find((x) => x.as === "net");
      must(listed?.op === "import" && listed.documents.some((d) => d.kind === "network") && listed.documents.some((d) => d.kind === "boundary"), "`session gate` does not list the import's documents");
      const replayed = JSON.parse(caps(["replay", "gate"], { env }).stdout);
      must(replayed.entries >= 5 && replayed.same === replayed.entries, "the journal did not replay the same");
      const journal = join(store, "gate", "journal.jsonl");
      const entries = readFileSync(journal, "utf8").trim().split("\n").map(JSON.parse);
      entries.at(-1).output = "sha256:" + "0".repeat(64);
      writeFileSync(journal, entries.map((x) => JSON.stringify(x)).join("\n") + "\n");
      caps(["replay", "gate"], { env, fails: true });
    } finally {
      rmSync(store, { recursive: true, force: true });
    }
  }],
  ["--brief prints a document as its kind, version, and size, and the session keeps it whole", () => {
    const r = caps(["network", "--brief", "--load", FEED,
      "--as", "net", "--op", '{"op":"import","feed_hash":"$load.feed_hash"}',
      "--cap", "board", "--as", "bnet", "--op", '{"op":"load_network","network":"$net.network"}']);
    const line = r.stdout.trim().split("\n")[r.lines.findIndex((x) => x.as === "net")];
    const net = r.named("net");
    const o = net.network.omitted;
    must(line.length <= 1000 && o?.kind === "network" && o.schema_version === 2 && o.bytes > 1000, "the network is not omitted by kind, version, and size");
    must(net.boundary.omitted?.kind === "boundary", "the boundary is not omitted");
    must(r.named("bnet").network_hash === net.network_hash, "the board did not get the whole network");
  }],
  ["the live board: import, cap-rt's live document, the board reads it by hash (08:01 at A: t10 expected 08:02)", () => {
    const r = caps([...LIVE_CHAIN, "--op", '{"op":"stop_board","network_hash":"$bnet.network_hash","stop_id":"A","date":"2026-10-23","from_secs":28860,"limit":3,"live_hash":"$blive.live_hash"}']);
    must(r.named("live").live_hash === r.named("blive").live_hash, "the board holds another live document");
    const row = r.last.ok.board.rows.find((x) => x.expected_secs != null);
    must(row?.expected_secs === 28920 && row.status === "late", "t10 is not late, expected 08:02");
  }],
  ["the stop display's grouped board over the live chain (10 to Downtown leads with t10, 1 min)", () => {
    const g = caps([...LIVE_CHAIN, "--op", '{"op":"grouped_board","network_hash":"$bnet.network_hash","stop_id":"A","date":"2026-10-23","from_secs":28860,"live_hash":"$blive.live_hash"}'])
      .last.ok?.board.groups.find((x) => x.route === "10");
    const d = g?.departures[0];
    must(d?.trip === "t10" && d.wait.kind === "minutes" && d.wait.minutes === 1 && g.departures.length <= 3, "route 10's group does not lead with t10 in 1 min");
  }],
  ["runcut: the day's blocks, one block's chain, the edit ripple (600 - 300)", () => {
    has(caps(["runcut", "--op", LOAD,
      "--op", '{"op":"blocks","network_hash":"$load_network.network_hash","date":"2026-10-23"}',
      "--op", `{"op":"load_edits","edits":${EDITS}}`,
      "--op", '{"op":"block_trips","network_hash":"$load_network.network_hash","edits_hash":"$load_edits.edits_hash","block_id":"B1","date":"2026-10-23"}',
    ]).stdout, '"layover_remaining_secs":300');
  }],
  ["blocking: the constraints, the recovery view, the suggestion adopted through author, the same fleet peak", () => {
    const constraints = '{"recovery":[{"min_secs":300}],"interline":[{"a":"10","b":"20"}]}';
    const r = caps(["runcut", "--op", LOAD,
      "--as", "wr", "--op", `{"op":"load_scheduling_constraints","scheduling_constraints":${constraints}}`,
      "--as", "rec", "--op", '{"op":"recovery","network_hash":"$load_network.network_hash","date":"2026-10-23","scheduling_constraints_hash":"$wr.scheduling_constraints_hash"}',
      "--as", "s", "--op", '{"op":"suggest_blocks","network_hash":"$load_network.network_hash","date":"2026-10-23","scheduling_constraints_hash":"$wr.scheduling_constraints_hash"}',
      "--cap", "author", "--op", LOAD,
      "--as", "awr", "--op", `{"op":"load_scheduling_constraints","scheduling_constraints":${constraints}}`,
      "--as", "a", "--op", '{"op":"apply","network_hash":"$load_network.network_hash","edits":"$s.suggestion.edits","scheduling_constraints_hash":"$awr.scheduling_constraints_hash"}',
      "--cap", "runcut", "--as", "adopted", "--op", '{"op":"load_network","network":"$a.network"}',
      "--as", "fleet", "--op", '{"op":"fleet_summary","network_hash":"$adopted.network_hash","date":"2026-10-23"}']);
    must(r.named("rec").recovery.waits.length > 0, "the recovery view has no waits");
    must(!r.named("a").issues.some((i) => i.layover_infeasible), "the adopted blocking is layover_infeasible");
    must(r.named("fleet").summary.peak_vehicles === r.named("s").suggestion.peak_vehicles, "the fleet peak is not the suggestion's");
  }],
  ["stops: validated, registered under validate's hash, read on a date, a return to service refused", () => {
    const r = caps(["stops",
      "--as", "v", "--op", `{"op":"validate","stops":${STOPS_DOC}}`,
      "--as", "l", "--op", `{"op":"load_stops","stops":${STOPS_DOC}}`,
      "--as", "spring", "--op", '{"op":"stop","stops_hash":"$l.stops_hash","stop_id":"6124","date":"2027-04-01"}',
      "--as", "back", "--op", '{"op":"stop","stops_hash":"$l.stops_hash","stop_id":"6124","date":"2027-07-01"}',
      "--as", "bad", "--op", '{"op":"validate","stops":{"stops":{"a":[{"event":"in_service","lat":45,"lon":-122},{"on":"2027-01-02","event":"in_service"}]}}}']);
    must(r.named("v").document_hash === r.named("l").stops_hash, "validate and load_stops answer other hashes");
    must(r.named("spring").stop.status === "suspended" && r.named("back").stop.status === "in_service", "the suspension's dates");
    must(r.named("bad").refusals[0]?.problem.kind === "already_in_service", "a return to service is not refused");
  }],
  ["requests: the first request begins the document, read back, listed, and ordered with no work order refused", () => {
    const r = caps(["requests",
      "--as", "first", "--op", '{"op":"put","id":"R-1","request":{"stop":"6124","job":"repair_improve","status":"submitted","requested_on":"2027-02-01"}}',
      "--as", "v", "--op", '{"op":"validate","requests":"$first.requests"}',
      "--as", "mine", "--op", '{"op":"requests_for","requests_hash":"$first.requests_hash","stop":"6124"}',
      "--as", "work", "--op", '{"op":"list","requests_hash":"$first.requests_hash","statuses":["submitted"]}',
      "--as", "bad", "--op", '{"op":"put","requests_hash":"$first.requests_hash","id":"R-1","request":{"stop":"6124","job":"repair_improve","status":"ordered"}}']);
    must(r.named("v").document_hash === r.named("first").requests_hash, "put and validate answer other hashes");
    must(r.named("mine").requests[0]?.id === "R-1" && r.named("work").requests[0]?.id === "R-1" && r.named("work").counts[0]?.requests === 1, "R-1 does not read back");
    must(r.named("bad").refusals[0]?.problem.kind === "no_work_order", "ordered with no work order is not refused");
  }],
  ["stops from a network: seeded, one suspension placed, and the nearest stop to its own point", () => {
    const r = caps(["stops", "--op", LOAD,
      "--as", "seed", "--op", '{"op":"seed","network_hash":"$load_network.network_hash"}',
      "--as", "edit", "--op", '{"op":"add_event","stops_hash":"$seed.stops_hash","stop_id":"A","event":{"on":"2026-10-23","event":"suspended","until":"2026-10-24"}}',
      "--as", "on", "--op", '{"op":"stop","stops_hash":"$edit.stops_hash","stop_id":"A","date":"2026-10-23"}',
      "--as", "near", "--op", '{"op":"stops_near","stops_hash":"$edit.stops_hash","date":"2026-10-23","lat":"$on.stop.lat","lon":"$on.stop.lon","limit":3}']);
    must(r.named("seed").stops.stops.A && r.named("edit").refusals.length === 0, "the seed or the edit");
    must(r.named("on").stop.status === "suspended", "A is not suspended on 10-23");
    const near = r.named("near").stops;
    must(near[0]?.stop_id === "A" && near[0].distance_m === 0 && near.length === 3, "A is not nearest its own point");
  }],
  ["the map's geometry is answered (W4): a plan's segment, the detoured trip, a route framed", () => {
    const r = caps(["router", "--op", LOAD,
      "--as", "plan", "--op", '{"op":"plan","network_hash":"$load_network.network_hash","from":"A","to":"B","date":"2026-10-23","depart_secs":25200,"max_legs":4}',
      "--op", '{"op":"itinerary_paths","network_hash":"$load_network.network_hash","date":"2026-10-23","itinerary":"$plan.itinerary"}',
      "--cap", "runcut", "--op", LOAD,
      "--as", "edits", "--op", `{"op":"load_edits","edits":${DIVERSION("accepted", '[{"lat":45.6,"lon":-73.6}]')}}`,
      "--as", "trip", "--op", '{"op":"trip_path","network_hash":"$load_network.network_hash","edits_hash":"$edits.edits_hash","trip":"t30","start_secs":31200,"date":"2026-10-23"}',
      "--cap", "directory", "--op", LOAD,
      "--as", "focus", "--op", '{"op":"focus","network_hash":"$load_network.network_hash","kind":"route","id":"10"}']);
    const legs = r.lines.find((x) => x.ok?.op === "itinerary_paths").ok.paths;
    must(legs[0]?.kind === "transit" && legs[0].path.length >= 2, "the plan's ridden segment");
    must(r.named("trip").path.some((p) => p.lat === 45.6 && p.lon === -73.6), "t30 is not drawn along the detour");
    must(r.named("focus").focus.lines.length === 3, "route 10 is not framed by its patterns");
  }],
  ["the planner's numbers and search (W5): composition, search, serving routes, timetable, the late notice", () => {
    const r = caps(["directory", "--op", LOAD,
      "--as", "detail", "--op", '{"op":"route_detail","network_hash":"$load_network.network_hash","route_id":"10"}',
      "--as", "stops", "--op", '{"op":"entities","network_hash":"$load_network.network_hash","kind":"stop","query":"main 2nd"}',
      "--as", "serving", "--op", '{"op":"stop_detail","network_hash":"$load_network.network_hash","stop_id":"A"}',
      "--as", "timetable", "--op", '{"op":"timetable","network_hash":"$load_network.network_hash","route_id":"10","direction":"0","members":["WK"]}',
      "--cap", "router", "--op", LOAD,
      "--as", "late", "--op", '{"op":"plan_compare","network_hash":"$load_network.network_hash","from":"A","to":"B","date":"2026-10-23","depart_secs":144000,"max_legs":6}']);
    const d = r.named("detail").detail;
    must(d.pattern_trips_by_type.map((g) => g.service_type).join() === "SUN,WK" && d.pattern_trips_by_type.every((g) => g.pattern_trips.every((s) => s.trips > 0)), "route 10's composition");
    must(r.named("stops").rows.map((s) => s.id).join() === "B", "the search is not narrowed by every term");
    must(r.named("serving").detail.routes.some((x) => x.id === "10"), "A's serving routes");
    must(r.named("timetable").timetable?.trips.length > 0, "route 10's timetable");
    must(r.named("late").notice?.text?.includes("within 6 legs"), "the plan with no trip has no notice");
  }],
  ["router: the plan, the diversion delta (5 min later), and the last bus home", () => {
    const r = caps(["router", "--op", LOAD,
      "--op", '{"op":"plan","network_hash":"$load_network.network_hash","from":"A","to":"B","date":"2026-10-23","depart_secs":25200,"max_legs":4}',
      "--op", `{"op":"load_edits","edits":${EDITS}}`,
      "--as", "cmp", "--op", '{"op":"plan_compare","network_hash":"$load_network.network_hash","edits_hash":"$load_edits.edits_hash","from":"A","to":"B","date":"2026-10-23","depart_secs":25200,"max_legs":4}',
      "--as", "last", "--op", '{"op":"plan_arrive_by","network_hash":"$load_network.network_hash","from":"A","to":"B","date":"2026-10-23","max_legs":4}']);
    const c = r.named("cmp");
    must(c.active?.arrival_hhmmss === "08:15:00" && c.notice?.kind === "later" && c.notice.minutes_lost === 5, "the diversion's delta");
    must(r.named("last").itinerary?.depart_secs === 31200, "the last bus home is not t30 at 08:40");
  }],
  ["the map's click-through (U6): the patched t30 keeps A to B", () => {
    has(caps(["runcut", "--op", LOAD,
      "--op", `{"op":"load_edits","edits":${EDITS}}`,
      "--op", '{"op":"trip_stops","network_hash":"$load_network.network_hash","edits_hash":"$load_edits.edits_hash","trip":"t30","start_secs":31200,"date":"2026-10-23"}',
    ]).stdout, '"stop_id":"B"');
  }],
  ["streets: imported, registered, two snaps, and the route between them along the streets", () => {
    has(caps(["streets", "--load", "web/e2e/fixtures/streets.geojson",
      "--op", '{"op":"load_streets","streets":"$load.streets"}',
      "--op", '{"op":"snap","streets_hash":"$2.streets_hash","lat":45.504,"lon":-73.503}',
      "--op", '{"op":"snap","streets_hash":"$2.streets_hash","lat":45.508,"lon":-73.508}',
      "--op", '{"op":"route","streets_hash":"$2.streets_hash","from":"$3.position","to":"$4.position"}',
    ]).stdout, '"lat":45.504,"lon":-73.508');
  }],
  ["walks: the walk table from streets, the router walks bay A to bay B (60 s)", () => {
    has(caps(["streets", "--load", "web/e2e/fixtures/walk-streets.geojson", "--op", LOAD,
      "--op", '{"op":"walks","streets_hash":"$load.streets_hash","network_hash":"$load_network.network_hash"}',
      "--cap", "router", "--op", LOAD,
      "--op", '{"op":"load_walks","walks":"$walks.walks"}',
      "--op", '{"op":"plan","network_hash":"$load_network.network_hash","from":"SA","to":"SB","date":"2026-10-23","depart_secs":18000,"max_legs":2,"walks_hash":"$load_walks.walks_hash"}',
    ]).stdout, '"arrival_secs":18060');
  }],
  ["places: a dropped pin's walks, and the router plans from the pin (137 s to bay B)", () => {
    has(caps(["streets", "--load", "web/e2e/fixtures/walk-streets.geojson", "--op", LOAD,
      "--op", '{"op":"access","streets_hash":"$load.streets_hash","network_hash":"$load_network.network_hash","point":{"lat":45.5152,"lon":-73.5165}}',
      "--cap", "router", "--op", LOAD,
      "--op", '{"op":"plan","network_hash":"$load_network.network_hash","from":{"id":"pin","name":"Dropped pin","walks":"$access.walks"},"to":"SB","date":"2026-10-23","depart_secs":18000,"max_legs":2}',
    ]).stdout, '"from_stop_id":"pin","from_stop_name":"Dropped pin","to_stop_id":"SB","to_stop_name":"Main Station: Bay B","depart_secs":18000,"arrival_secs":18137');
  }],
  ["sync: a draft client against an empty server converges to submitted", () => {
    const client = JSON.parse(DIVERSION("draft", "null")).records;
    has(caps(["sync", "--op", JSON.stringify({ op: "sync_edits", client, server: [], now_ms: 1000 })]).stdout, '"status":"submitted"');
  }],
  ["authz: a matching rule allows, an empty policy denies", () => {
    has(caps(["authz", "--op", '{"op":"authorize","identity":{"sub":"d","roles":["dispatcher"]},"action":"write","pointer":"edits","policy":{"schema_version":1,"rules":[{"name":"ops","roles":["dispatcher"],"pointers":["edits"],"actions":["write"]}]}}']).stdout, '"allow":true');
    has(caps(["authz", "--op", '{"op":"authorize","identity":{"sub":"r","roles":["rider"]},"action":"write","pointer":"edits","policy":{"schema_version":1,"rules":[]}}']).stdout, '"allow":false');
  }],
  ["plans: the fall pick governs after the spring one", () => {
    const plans = '[{"id":"spring","name":"Spring 2026","effective_from":"2026-03-01"},{"id":"fall","name":"Fall 2026","effective_from":"2026-09-07"}]';
    has(caps(["plans", "--op", `{"op":"resolve","plans":${plans},"date":"2026-10-23"}`, "--op", `{"op":"periods","plans":${plans}}`]).stdout, '"plan":"fall"');
  }],
  ["author: the network stamped as a pick, read by the calendar by the hash author answered", () => {
    has(caps(["author", "--op", LOAD,
      "--as", "pick", "--op", '{"op":"apply","network_hash":"$load_network.network_hash","edits":[{"edit":"set_plan","plan":{"id":"fall","name":"Fall 2026","effective_from":"2026-10-19"}}]}',
      "--cap", "calendar", "--op", '{"op":"load_network","network":"$pick.network"}',
      "--op", '{"op":"active_services","network_hash":"$pick.network_hash","date":"2026-10-23"}',
    ]).stdout, '"services":["WK"]');
  }],
  ["divert: a draft proposed into an empty edit document reports valid", () => {
    has(caps(["divert", "--op", LOAD,
      "--as", "prop", "--op", '{"op":"propose","network_hash":"$load_network.network_hash","draft":{"route_id":"10","pattern":"pat-10-0-1","first_kept_stop":"A","last_kept_stop":"B","served_stops":[],"slices":[],"added_mins":5,"start_date":"2026-10-23","end_date":"2026-10-23"},"edits":{"kind":"edit_records","schema_version":2,"records":[]},"record_id":"e1","now_ms":1000}',
      "--op", '{"op":"reports","network_hash":"$load_network.network_hash","edits":"$prop.edits"}',
    ]).stdout, '"valid":true');
  }],
  ["a trip cancel: divert cancels t10, the board lists its visit cancelled (D-100)", () => {
    has(caps(["divert", "--op", LOAD,
      "--as", "c", "--op", '{"op":"cancel","network_hash":"$load_network.network_hash","edits":{"kind":"edit_records","schema_version":2,"records":[]},"record_id":"c1","trip_id":"t10","date":"2026-10-23","now_ms":1000}',
      "--cap", "board", "--op", LOAD,
      "--op", '{"op":"load_edits","edits":"$c.edits"}',
      "--op", '{"op":"stop_board","network_hash":"$load_network.network_hash","edits_hash":"$load_edits.edits_hash","stop_id":"A","date":"2026-10-23","from_secs":25200,"limit":10}',
    ]).stdout, '"cancel":"cancel-t10-1000"');
  }],
];

export async function runSessions(sessions, log = console.error) {
  let failed = 0;
  for (const [name, run] of sessions) {
    try {
      await run();
    } catch (e) {
      failed++;
      log(`SESSION GATE FAIL (tools/gates/sessions.mjs): ${name}: ${e instanceof SessionFailed ? e.message : e.stack}`);
    }
  }
  return failed;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exitCode = (await runSessions(SESSIONS)) ? 1 : 0;
}
