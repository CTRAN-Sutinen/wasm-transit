// caps-run is the agent and human runner for the capability registry. It
// pulls a module by its pinned hash, verifies the bytes, and runs ops in
// the sandbox (no imports), with a wall-clock guard that terminates the
// worker.
//
// A human answers the same questions that an agent can, through the same
// calls. The registry record's op_docs are the tool descriptions, the
// worked examples are the few-shot examples, and the output hashes chain
// ops the way an agent's context does.
//
//   caps-run list                       capabilities + evidence
//   caps-run session [NAME]             the sessions in the store, or
//                                       what one session holds
//   caps-run replay NAME|FILE.json [--single] [--module CAP=FILE]...
//                                       replay a session's journal, or a
//                                       page's, and compare every output
//                                       hash
//   caps-run tools <capability> <op>    one op: its description and
//                                       its worked example
//   caps-run tools <capability> --brief each op's first sentence and
//                                       its example request, with the
//                                       documents omitted
//   caps-run tools <capability>         the tool catalog (op docs, and
//                                       per-kind examples where an op
//                                       takes a tagged vocabulary)
//   caps-run <capability> [--session NAME] [--brief] [--tz ZONE] [--cap ID] [--as NAME]
//          [--load [N:|KIND:]FILE]... [--op '<json>|@FILE']... [--ask URL]...
//          [--post URL REF]... [--timeout MS]   run a session
//
// --session NAME keeps the session across commands. Each result named
// with --as is saved in the session's store (.caps-sessions/NAME/, or
// $CAPS_RUN_SESSIONS/NAME/). A later command with the same --session
// reaches it as $name, without running its step again. Only --as names
// reach across commands. Save happens only when every step succeeds.
// The session also keeps a journal of every module call: hashes only,
// with each request's bytes stored once by hash (tools/journal.mjs).
// `replay NAME` runs it again on fresh instances and reports each
// output that differs. `--module CAP=FILE` replays CAP against a
// candidate module.
// Each command makes new module instances. A stored document registers
// itself the first time an op names its hash (below).
//
// --brief prints each whole document (an object that carries `kind`
// and `schema_version`) as {"omitted": {kind, schema_version, bytes}}.
// The session keeps the whole document, so references still reach it.
//
// Every run compares the registry's recorded source digest with the
// tree's and says on stderr when they differ. The registry runs the code
// that it was built from, not the code in the tree (`just caps`).
//
// --load [N:|KIND:]FILE sends the file's bytes on a bulk op. KIND names
// the kernel's load of a document kind (`--load feed:feed.zip`,
// `layer:jobs.geojson`, `rt:snapshot.pb`, `network:network.json.gz`,
// `streets:streets.json`). These are the registry's kernel_ops, in every
// module whose capabilities read the kind. N is the op code. The default
// codes count from 0 over the capability's own bulk ops (streets' import,
// rt's decode). Every module also answers the registry kernel ops:
// `--load 258:/dev/null` lists what it holds, `--load 257:keep.json`
// drops everything that `{"keep": [hash, …]}` does not name, and
// `--load 268:/dev/null` says what it links.
// --op invokes one JSON op and is repeatable. '@FILE' reads the JSON from
// a file. Steps run in command-line order, loads and ops alike, and
// results print as JSON, one per line, in that order.
//
// --ask URL asks an oracle through the deployment's path, as a page does.
// An oracle is an external service whose answer is an input that the user
// could have given by hand (docs/architecture/web.md, "External
// services"): `--ask 'http://127.0.0.1:8080/geocode/
// autocomplete?text=peacehealth'`. Its JSON answer joins the session as a
// result (`$ask.features.0.geometry.coordinates.1`), so "the rider typed
// X and picked the first" chains into the ops.
//
// --post URL REF sends a session document to the deployment, as a page
// writes one, and joins its answer as a result ($post). REF is a result
// reference (below), or a JSON value whose references resolve inside it
// (`'{"id":"fall-2026"}'`), POSTed as JSON.
// `--post http://127.0.0.1:8080/plans '$apply.network'` publishes the
// pick that an `apply` answered (server/openapi.json, POST /plans). A
// refusal (4xx) is the step's error, carrying the server's answer.
//
// A session spans capabilities. --cap ID switches the capability that the
// following steps run against (each module is verified and instantiated
// once per session). Documents cross by reference:
// `"network":"$import.network"` hands the imported document to the next
// capability, the same bytes, with no file in between. Assert that the
// identities agree, as any host does.
//
// Result references: a string value that starts with "$" resolves
// against the session's prior results before the op runs, so nobody
// copies hashes between ops by hand:
//   $net.network_hash  the result named by `--as net` (names are the
//                      stable address and survive reordering)
//   $load.feed_hash    the last --load result ("load" op)
//   $ask.features      the last --ask result (the oracle's answer)
//   $post.plans_hash   the last --post result (the deployment's answer)
//   $import.network_hash  the last prior result tagged op:"import"
//   $1.feed_hash       the 1st result overall, in command-line order
// The rest of the reference is a dotted path into the result (numeric
// segments index arrays). A reference must be the whole string value.
// An unresolvable reference fails the session.
//
// The host owns the clock and reads it in the agency's zone, never the
// machine's (D-175). The zone is --tz ZONE, or else the zone of the last
// network the session saw (its operators' zone, or an import's
// `timezone`). With no zone, $TODAY and $NOW_SECS fail.
//   $TODAY             the agency's service date, "YYYY-MM-DD"
//   $NOW_SECS          seconds from noon minus 12 h on that date (a
//                      number; the seconds fields are u32). This is
//                      web/src/clock.ts, the browser's own clock.
//
// A document that the module does not hold, named by a hash that the
// session holds, registers itself: the runner loads it through the kind's
// kernel load and runs the op again. The line lists it in `registered`.

import { createHash, randomUUID } from "node:crypto";
import { UNKNOWN_DOCUMENT, journalWriter, readJournal, replay, unpackJournal } from "./journal.mjs";
import { parentPort, workerData, isMainThread } from "node:worker_threads";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { gunzipSync, gzipSync } from "node:zlib";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REGISTRY = join(HERE, "..", "registry");

function registryPointer() {
  return JSON.parse(readFileSync(join(REGISTRY, "registry.json"), "utf8"));
}

function kindLoads() {
  return Object.fromEntries((registryPointer().kernel_ops ?? []).filter((k) => k.kind).map((k) => [k.kind, k]));
}

function opCode(prefix) {
  if (/^\d+$/.test(prefix)) return Number(prefix);
  const loads = kindLoads();
  const load = loads[prefix];
  if (!load) throw new Error(`--load ${prefix}:FILE — no kernel load for a ${prefix} document (kinds: ${Object.keys(loads).join(", ")})`);
  return load.code;
}

// The digest must match what gen-manifest records (`source_digest`): every
// file that git sees under crates/ plus Cargo.toml and Cargo.lock, sha256
// over "{path}\n{sha256 hex}\n" in path order. It is null without git.
function sourceDigest() {
  const root = join(HERE, "..");
  let listing;
  try {
    listing = execFileSync("git", ["ls-files", "-co", "--exclude-standard", "-z", "--", "crates", "Cargo.toml", "Cargo.lock"], {
      cwd: root,
      maxBuffer: 64 * 1024 * 1024,
    }).toString("utf8");
  } catch {
    return null;
  }
  const paths = [...new Set(listing.split("\0").filter(Boolean))].sort();
  const digest = createHash("sha256");
  for (const path of paths) {
    const file = join(root, path);
    if (!existsSync(file)) continue;
    digest.update(`${path}\n${createHash("sha256").update(readFileSync(file)).digest("hex")}\n`);
  }
  return "sha256:" + digest.digest("hex");
}

// A registry published from another tree runs other code than the tree
// holds, so the runner says so. In a session it says so once for each
// registry, because the session's directory remembers the digest.
function warnIfStale(session = null) {
  const published = registryPointer().evidence?.source_digest ?? null;
  const tree = sourceDigest();
  if (!published || !tree || published === tree) return;
  if (session) {
    const marker = join(sessionDir(session), ".stale-warned");
    if (existsSync(marker) && readFileSync(marker, "utf8") === published) return;
    mkdirSync(sessionDir(session), { recursive: true });
    writeFileSync(marker, published);
  }
  console.error(
    `caps-run: the registry is STALE — published from source ${published.slice(0, 19)}…, ` +
      `the tree is ${tree.slice(0, 19)}…; \`just caps\` republishes it`,
  );
}

function capabilityRecord(id) {
  return JSON.parse(readFileSync(join(REGISTRY, "capabilities", `${id}.json`), "utf8"));
}

function sha256(bytes) {
  return "sha256:" + createHash("sha256").update(bytes).digest("hex");
}

function usage() {
  console.error(
    "usage: caps-run list | caps-run tools <capability> [<op>] [--brief] | caps-run session [NAME] | " +
      "caps-run replay NAME|FILE.json [--single] [--module CAP=FILE]... | " +
      "caps-run <capability> [--session NAME] [--brief] [--tz ZONE] [--cap ID] [--as NAME] [--load FILE]... [--op JSON|@FILE]... [--ask URL]... [--post URL REF]... [--timeout MS]",
  );
  process.exit(2);
}

function parseArgs(argv) {
  const out = { capability: null, steps: [], timeout: 10_000, session: null, brief: false, tz: null };
  let current = null;
  let pendingName = null;
  const step = (kind, value) => {
    if (current === null) usage();
    out.steps.push({ kind, value, capability: current, name: pendingName });
    pendingName = null;
  };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--load") step("load", argv[++i]);
    else if (arg === "--op") step("op", argv[++i]);
    else if (arg === "--ask") step("ask", argv[++i] ?? usage());
    else if (arg === "--post") step("post", { url: argv[++i] ?? usage(), ref: argv[++i] ?? usage() });
    else if (arg === "--cap") current = argv[++i] ?? usage();
    else if (arg === "--as") pendingName = argv[++i] ?? usage();
    else if (arg === "--timeout") out.timeout = Number(argv[++i]);
    else if (arg === "--brief") out.brief = true;
    else if (arg === "--tz") {
      out.tz = argv[++i] ?? usage();
      try {
        new Intl.DateTimeFormat("en", { timeZone: out.tz });
      } catch {
        console.error(`--tz ${out.tz}: not a time zone the host knows (an IANA name, such as America/Los_Angeles)`);
        process.exit(2);
      }
    }
    else if (arg === "--session") {
      out.session = argv[++i] ?? usage();
      if (!/^[A-Za-z0-9._-]+$/.test(out.session) || out.session.startsWith(".")) {
        console.error(`--session ${out.session}: a session name uses letters, digits, ".", "_", and "-", and does not start with "."`);
        process.exit(2);
      }
    }
    else if (arg.startsWith("--")) usage();
    else if (out.capability === null) out.capability = current = arg;
    else usage();
  }
  if (!out.capability || pendingName !== null) usage();
  return out;
}

// The session runs in a worker thread, so the parent can terminate a
// runaway computation. That is the wall-clock half of metering. The
// no-import wasm sandbox is the other half.

// The bytes are checked against the registry record's pin before anything
// runs. `file` replaces the registry's blob with a candidate module
// (replay --module).
function instantiate(capability, { file = null, journal = null } = {}) {
  const record = capabilityRecord(capability);
  const bytes = readFileSync(file ?? join(REGISTRY, record.blob));
  const hash = sha256(bytes);
  if (!file && hash !== record.hash) {
    throw new Error(`registry integrity: blob is ${hash}, record pins ${record.hash}`);
  }
  const mod = new WebAssembly.Module(bytes);
  const imports = WebAssembly.Module.imports(mod);
  if (imports.length !== 0) throw new Error(`capability ${capability} has imports — refusing`);
  const ex = new WebAssembly.Instance(mod, {}).exports;

  // The outcome stays the module's JSON text. A session holds every
  // result for later references, and a parsed real-size answer is millions
  // of objects that every major collection marks again while the next
  // large answer runs.
  const read = (outPtr) => {
    const len = new DataView(ex.memory.buffer).getUint32(outPtr, true);
    const outcome = Buffer.from(ex.memory.buffer, outPtr + 4, len).toString("utf8");
    ex.transit_free(outPtr, len + 4);
    return outcome;
  };
  const call = (payload, enter) => {
    const ptr = ex.transit_alloc(payload.length);
    new Uint8Array(ex.memory.buffer, ptr, payload.length).set(payload);
    const outPtr = enter(ptr, payload.length);
    ex.transit_free(ptr, payload.length);
    return read(outPtr);
  };
  const json = (payload) => {
    const outcome = call(payload, (ptr, len) => ex.transit_invoke(ptr, len));
    journal?.({ module: hash, call: "json", op: JSON.parse(payload.toString("utf8")).op ?? null, payload, outcome });
    return outcome;
  };
  const bulk = (op, payload) => {
    const outcome = call(payload, (ptr, len) => ex.transit_invoke_bytes(op, ptr, len));
    journal?.({ module: hash, call: op, op: bulkOpName(op), payload, outcome });
    return outcome;
  };
  return {
    module: hash,
    invokeBytes: (op, payload) => bulk(op, Buffer.isBuffer(payload) ? payload : Buffer.from(payload)),
    invoke: (request) => json(Buffer.from(JSON.stringify(request))),
    invokeRaw: json,
  };
}

// A page's journal replays on the bytes that the page ran.
function linkedAssemblies() {
  const caps = join(HERE, "..", "web", "public", "caps");
  const file = join(caps, "manifest.json");
  if (!existsSync(file)) return new Map();
  const manifest = JSON.parse(readFileSync(file, "utf8"));
  return new Map(
    (manifest.assemblies ?? [])
      .filter((a) => a.capabilities.length > 1 && existsSync(join(caps, "..", a.url)))
      .map((a) => [a.hash, join(caps, "..", a.url)]),
  );
}

function bulkOpName(code) {
  return (registryPointer().kernel_ops ?? []).find((k) => k.code === code)?.name ?? `bulk ${code}`;
}

// index.json lists the named results. Each result is its outcome text,
// gzipped, in <name>.json.gz.
function sessionDir(name) {
  return join(process.env.CAPS_RUN_SESSIONS ?? join(HERE, "..", ".caps-sessions"), name);
}

function readSessionIndex(name) {
  const file = join(sessionDir(name), "index.json");
  return existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : [];
}

// Every persisted document carries `kind` and `schema_version` inside it
// (docs/architecture/bus.md, "Every document says what it is").
const isDocument = (value) =>
  value !== null && typeof value === "object" && typeof value.kind === "string" && typeof value.schema_version === "number";

// A document is a top-level field with its hash beside it (`network` and
// `network_hash`).
function documentsOf(ok) {
  if (!ok || typeof ok !== "object") return [];
  return Object.entries(ok)
    .filter(([, value]) => isDocument(value))
    .map(([field, value]) => ({ kind: value.kind, field, hash: ok[`${field}_hash`] ?? null }));
}

// An import or a document answer also states the zone as `timezone`.
function zoneOf(value) {
  if (!value || typeof value !== "object") return null;
  if (typeof value.timezone === "string") return value.timezone;
  if (value.kind === "network" && value.operators) {
    const zones = [...new Set(Object.values(value.operators).map((o) => o?.timezone).filter(Boolean))];
    return zones.length === 1 ? zones[0] : null;
  }
  for (const field of ["network", "document"]) {
    if (value[field]?.kind === "network") return zoneOf(value[field]);
  }
  return null;
}

// The index is written last, through a rename, so a reader never sees an
// index that names a missing file. A name from an earlier command is
// replaced.
function saveSession(name, named) {
  if (named.length === 0) return;
  const dir = sessionDir(name);
  mkdirSync(dir, { recursive: true });
  const index = readSessionIndex(name).filter((e) => !named.some((r) => r.as === e.as));
  for (const r of named) {
    writeFileSync(join(dir, `${r.as}.json.gz`), gzipSync(r.text));
    const ok = JSON.parse(r.text).ok;
    const entry = { as: r.as, cap: r.capability, op: ok?.op ?? null, documents: documentsOf(ok) };
    const zone = r.zone ?? zoneOf(ok);
    if (zone) entry.timezone = zone;
    index.push(entry);
  }
  writeFileSync(join(dir, "index.json.tmp"), JSON.stringify(index, null, 2));
  renameSync(join(dir, "index.json.tmp"), join(dir, "index.json"));
}

function briefValue(value) {
  if (Array.isArray(value)) return value.map(briefValue);
  if (value === null || typeof value !== "object") return value;
  if (isDocument(value)) {
    return { omitted: { kind: value.kind, schema_version: value.schema_version, bytes: Buffer.byteLength(JSON.stringify(value)) } };
  }
  return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, briefValue(v)]));
}

function briefText(text) {
  const outcome = JSON.parse(text);
  if (outcome.ok && typeof outcome.ok === "object") {
    outcome.ok = Object.fromEntries(Object.entries(outcome.ok).map(([k, v]) => [k, briefValue(v)]));
  }
  return JSON.stringify(outcome);
}

async function runSession() {
  const { steps, session, brief, tz } = workerData;
  // This is web/src/clock.ts, which has no imports.
  const { serviceClock } = await import("../web/src/clock.ts");
  const modules = new Map();
  const run = `${new Date().toISOString()}-${randomUUID().slice(0, 8)}`;
  const journal = session ? journalWriter(sessionDir(session), run) : null;
  const moduleFor = (capability) => {
    if (!modules.has(capability)) {
      const record = journal && ((call) => journal.record({ instance: `${run}/${capability}`, cap: capability, ...call }));
      modules.set(capability, instantiate(capability, { journal: record }));
    }
    return modules.get(capability);
  };

  // A result's outcome stays JSON text until a reference reaches into it.
  // With --session, the named results of earlier commands come first, and
  // each one's text is read from the store only when a reference reaches
  // it. $load, $ask, $post, $op, and $N address this command's results
  // only.
  const results = session
    ? readSessionIndex(session).map((e) => ({
        as: e.as,
        op: e.op,
        prior: true,
        zone: e.timezone,
        documents: e.documents,
        get text() {
          const text = gunzipSync(readFileSync(join(sessionDir(session), `${e.as}.json.gz`))).toString("utf8");
          Object.defineProperty(this, "text", { value: text });
          return text;
        },
      }))
    : [];
  const outcomeOf = (r) => JSON.parse(r.text);
  const opOf = (r) => {
    if (r.op === undefined) r.op = outcomeOf(r).ok?.op ?? null;
    return r.op;
  };
  // The host owns the clock, so $TODAY and $NOW_SECS resolve before the
  // ops see them, in the agency's zone and never the machine's (D-175).
  // Without --tz, the zone comes from the last network that the session
  // has seen: one that an op loads, one that an import or a document
  // answer names, or one that an earlier command saved.
  const nowMs = Date.now();
  let zone = tz ?? [...results].reverse().find((r) => r.zone)?.zone ?? null;
  const learnZone = (value, result) => {
    if (tz) return;
    const found = zoneOf(value);
    if (found) {
      zone = found;
      Object.defineProperty(result, "zone", { value: found, configurable: true });
    }
  };
  const clockRef = (name) => {
    if (!zone) {
      throw new Error(`${name} needs the agency's time zone: pass --tz ZONE, or load or import a network in the session first`);
    }
    const clock = serviceClock(nowMs, zone);
    return name === "$TODAY" ? clock.date : clock.secs;
  };
  const clockRefs = { $TODAY: true, $NOW_SECS: true };
  const loadCodes = Object.fromEntries((registryPointer().kernel_ops ?? []).map((k) => [k.name, k.code]));
  const listedDocuments = new Map();
  const heldDocument = (kind, hash) => {
    for (const r of [...results].reverse()) {
      if (!listedDocuments.has(r)) listedDocuments.set(r, r.documents ?? documentsOf(outcomeOf(r).ok));
      const listed = listedDocuments.get(r);
      const found = listed.find((d) => d.kind === kind && d.hash === hash);
      if (found) return outcomeOf(r).ok[found.field];
    }
    return null;
  };
  const resolveRef = (ref) => {
    const m = /^\$(load|ask|post|\d+|[a-zA-Z_][a-zA-Z0-9_]*)(?:\.([a-zA-Z0-9_.]*))?$/.exec(ref);
    if (!m) throw new Error(`bad result reference ${JSON.stringify(ref)}: expected $<name|load|ask|post|op|N>.<path>`);
    const [, base, path] = m;
    let current;
    const named = [...results].reverse().find((r) => r.as === base);
    const here = results.filter((r) => !r.prior);
    if (named) {
      current = named;
    } else if (base === "load") {
      current = [...here].reverse().find((r) => r.load !== undefined);
      if (!current) throw new Error(`reference ${ref}: no --load result to resolve against`);
    } else if (base === "ask") {
      current = [...here].reverse().find((r) => r.ask !== undefined);
      if (!current) throw new Error(`reference ${ref}: no --ask result to resolve against`);
    } else if (base === "post") {
      current = [...here].reverse().find((r) => r.post !== undefined);
      if (!current) throw new Error(`reference ${ref}: no --post result to resolve against`);
    } else if (/^\d+$/.test(base)) {
      current = here[Number(base) - 1];
      if (!current) throw new Error(`reference ${ref}: no result ${base} (the session has ${here.length})`);
    } else {
      current = [...here].reverse().find((r) => opOf(r) === base);
      if (!current) throw new Error(`reference ${ref}: no prior result tagged op:"${base}"`);
    }
    // References resolve into the result's ok payload. The outcome wrapper
    // is transport.
    const outcome = outcomeOf(current);
    current = outcome.ok ?? outcome;
    for (const segment of (path ?? "").split(".").filter((s) => s.length)) {
      if (current === null || current === undefined) {
        throw new Error(`reference ${ref}: path leaves the result at segment "${segment}"`);
      }
      current = current[segment];
    }
    if (current === null || current === undefined) {
      throw new Error(`reference ${ref}: resolves to nothing`);
    }
    return current;
  };
  const resolveRefs = (value) => {
    if (typeof value === "string") {
      if (clockRefs[value] !== undefined) return clockRef(value);
      if (value.startsWith("$")) return resolveRef(value);
      return value;
    }
    if (Array.isArray(value)) return value.map(resolveRefs);
    if (value !== null && typeof value === "object") {
      for (const key of Object.keys(value)) value[key] = resolveRefs(value[key]);
      return value;
    }
    return value;
  };
  for (const step of steps) {
    const result = {};
    if (step.name) {
      if (results.some((r) => r.as === step.name && !r.prior)) throw new Error(`--as ${step.name}: the name is already taken`);
      result.as = step.name;
    }
    // Once a session spans more than one capability, every line names the
    // module that answered.
    if (new Set(steps.map((s) => s.capability)).size > 1) result.cap = step.capability;
    if (step.kind === "ask") {
      // The oracle answers over HTTP, outside the sandbox. Its answer is
      // input, like a file that the session loads.
      const resp = await fetch(step.value);
      result.ask = step.value;
      result.text = resp.ok ? `{"ok":${JSON.stringify(await resp.json())}}` : JSON.stringify({ error: `HTTP ${resp.status}` });
    } else if (step.kind === "post") {
      const raw = step.value.ref;
      const value = raw.startsWith("{") || raw.startsWith("[") ? JSON.parse(raw) : raw;
      const body = JSON.stringify(resolveRefs(value));
      const resp = await fetch(step.value.url, { method: "POST", headers: { "Content-Type": "application/json" }, body });
      const text = await resp.text();
      let answer;
      try {
        answer = JSON.parse(text);
      } catch {
        answer = text.trim();
      }
      result.post = step.value.url;
      result.text = resp.ok ? JSON.stringify({ ok: answer }) : JSON.stringify({ error: `HTTP ${resp.status}`, answer });
    } else if (step.kind === "load") {
      const module = moduleFor(step.capability);
      const match = /^(\d+|[a-z_]+):(.+)$/.exec(step.value);
      const op = match ? opCode(match[1]) : 0;
      const file = match ? match[2] : step.value;
      result.load = file;
      result.text = module.invokeBytes(op, readFileSync(file));
    } else {
      const module = moduleFor(step.capability);
      const request = step.value.startsWith("@")
        ? JSON.parse(readFileSync(step.value.slice(1), "utf8"))
        : JSON.parse(step.value);
      const resolved = resolveRefs(request);
      for (const value of Object.values(resolved)) learnZone(value, result);
      result.text = module.invoke(resolved);
      for (let tries = 0; tries < 8; tries++) {
        // The kernel's label for the kind is free text ("live document").
        // The load op's name is exact.
        const missing = UNKNOWN_DOCUMENT.exec(result.text);
        const kind = missing && Object.values(kindLoads()).find((k) => k.name === missing[3])?.kind;
        const document = kind && heldDocument(kind, missing[2]);
        if (!document || loadCodes[missing[3]] === undefined) break;
        const loaded = JSON.parse(module.invokeBytes(loadCodes[missing[3]], Buffer.from(JSON.stringify(document))));
        // A load names its hash field for the kind's short name
        // (`alerts_hash` for service_alerts), so match any hash field.
        const named = Object.entries(loaded.ok ?? {}).some(([field, value]) => field.endsWith("_hash") && value === missing[2]);
        if (!named) {
          throw new Error(`registering ${kind} ${missing[2]}: the module answered ${JSON.stringify(loaded).slice(0, 200)}`);
        }
        (result.registered ??= []).push(`${kind} ${missing[2]}`);
        result.text = module.invoke(resolved);
      }
    }
    if (!tz && result.text?.includes('"timezone"') && result.text.length < 64 * 1024 * 1024) {
      const ok = JSON.parse(result.text).ok;
      learnZone(ok, result);
    }
    Object.defineProperty(result, "capability", { value: step.capability });
    results.push(result);
  }
  const here = results.filter((r) => !r.prior);
  if (session) {
    journal.flush();
    saveSession(session, here.filter((r) => r.as));
  }
  // Without --brief the outcome's fields are spliced as text and never
  // parsed again.
  const lines = here.map(({ text, op, ...lead }) => {
    const head = JSON.stringify(lead);
    const body = brief ? briefText(text) : text;
    return head === "{}" ? body : `${head.slice(0, -1)},${body.slice(1)}`;
  });
  parentPort.postMessage({ ok: lines });
}

if (!isMainThread) {
  runSession().catch((error) => {
    parentPort.postMessage({ error: String(error) });
  });
}


export async function main(argv) {
  if (argv[0] === "list") {
    warnIfStale();
    const pointer = registryPointer();
    const kernel_ops = (pointer.kernel_ops ?? []).map(({ code, name, description }) => ({ code, name, description }));
    console.log(JSON.stringify({ ...pointer, kernel_ops }, null, 2));
    return 0;
  }
  if (argv[0] === "tools") {
    warnIfStale();
    const args = argv.slice(1);
    const brief = args.includes("--brief");
    const [id, only] = args.filter((a) => a !== "--brief");
    const record = capabilityRecord(id ?? usage());
    const docOf = (op) => record.op_docs[op] ?? { description: null, example: null };
    if (only !== undefined) {
      if (!record.ops.includes(only)) {
        console.error(`${record.id} has no op ${only}; its ops: ${record.ops.join(", ")}`);
        return 1;
      }
      console.log(JSON.stringify({ id: record.id, op: only, documents: record.documents ?? [], ...docOf(only) }, null, 2));
      return 0;
    }
    const firstSentence = (text) => {
      const match = /^(.+?[.!?])(?=\s+[A-Z(`"]|$)/s.exec(text ?? "");
      const sentence = (match ? match[1] : text ?? "").replace(/\s+/g, " ").trim();
      return sentence.length > 300 ? `${sentence.slice(0, 297)}…` : sentence;
    };
    const tools = Object.fromEntries(
      record.ops.map((op) => {
        const doc = docOf(op);
        if (!brief) return [op, doc];
        return [op, { summary: firstSentence(doc.description), example: { request: briefValue(doc.example?.request ?? null) } }];
      }),
    );
    const reads = new Set(record.documents ?? []);
    const kernel = (registryPointer().kernel_ops ?? []).filter((k) => reads.has(k.kind));
    const loads = brief
      ? Object.fromEntries(kernel.map((k) => [k.name, k.code]))
      : Object.fromEntries(kernel.map((k) => [k.name, k]));
    console.log(JSON.stringify({ id: record.id, purpose: record.purpose, documents: record.documents ?? [], loads, tools }, null, 2));
    return 0;
  }
  if (argv[0] === "replay") {
    // NAME is a session in the store, or a page's journal file (FILE.json,
    // from the engine's journal). Each entry runs on the module it
    // recorded when the runner has those bytes (a capability's registry
    // blob, or a page's linked assembly in web/public/caps), else on the
    // capability's own module. --single runs every entry on the
    // capability's own module, which checks assembly independence.
    const name = argv[1] ?? usage();
    const candidates = {};
    let single = false;
    for (let i = 2; i < argv.length; i++) {
      if (argv[i] === "--single") {
        single = true;
        continue;
      }
      const m = argv[i] === "--module" && /^([a-z_]+)=(.+)$/.exec(argv[i + 1] ?? "");
      if (!m) usage();
      candidates[m[1]] = m[2];
      i++;
    }
    let dir = sessionDir(name);
    if (name.endsWith(".json") && existsSync(name)) {
      dir = mkdtempSync(join(tmpdir(), "caps-run-replay-"));
      unpackJournal(dir, JSON.parse(readFileSync(name, "utf8")));
    } else if (!existsSync(join(dir, "journal.jsonl"))) {
      console.error(`no journal in session ${name} (${dir})`);
      return 1;
    } else {
      warnIfStale(name);
    }
    const kernelLoads = Object.fromEntries((registryPointer().kernel_ops ?? []).filter((k) => k.name.startsWith("load_")).map((k) => [k.name, k.code]));
    const assemblies = linkedAssemblies();
    const place = (cap, recorded) => {
      if (candidates[cap]) return { linked: false, open: () => instantiate(cap, { file: candidates[cap] }) };
      const linked = !single && assemblies.get(recorded);
      if (linked) return { linked: true, open: () => instantiate(cap, { file: linked }) };
      return { linked: false, open: () => instantiate(cap) };
    };
    const stateOps = Object.fromEntries((registryPointer().kernel_ops ?? []).filter((k) => k.name === "evict" || k.name === "held").map((k) => [k.name, k.code]));
    const report = replay({ dir, kernelLoads, stateOps, place });
    if (dir !== sessionDir(name)) rmSync(dir, { recursive: true, force: true });
    console.log(JSON.stringify({ journal: name, ...report }, null, 2));
    return report.differ.length === 0 && report.cannot.length === 0 ? 0 : 1;
  }
  if (argv[0] === "session") {
    const root = process.env.CAPS_RUN_SESSIONS ?? join(HERE, "..", ".caps-sessions");
    if (argv[1] === undefined) {
      const names = existsSync(root)
        ? readdirSync(root).filter((n) => existsSync(join(root, n, "index.json")) || existsSync(join(root, n, "journal.jsonl")))
        : [];
      const listing = (name) => ({ name, results: readSessionIndex(name).length, journal: readJournal(sessionDir(name)).length });
      console.log(JSON.stringify({ store: root, sessions: names.map(listing) }, null, 2));
      return 0;
    }
    const dir = sessionDir(argv[1]);
    if (!existsSync(join(dir, "index.json")) && !existsSync(join(dir, "journal.jsonl"))) {
      console.error(`no session ${argv[1]} in ${root}`);
      return 1;
    }
    console.log(JSON.stringify({ session: argv[1], dir, results: readSessionIndex(argv[1]), journal: readJournal(dir).length }, null, 2));
    return 0;
  }

  const { steps, timeout, session, brief, tz } = parseArgs(argv);
  warnIfStale(session);
  for (const capability of new Set(steps.map((s) => s.capability))) capabilityRecord(capability);
  const { Worker } = await import("node:worker_threads");
  const worker = new Worker(new URL(import.meta.url), {
    workerData: { steps, session, brief, tz },
  });

  const outcome = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      worker.terminate().then(() =>
        reject(new Error(`session exceeded ${timeout} ms — terminated (metering)`)),
      );
    }, timeout);
    worker.on("message", (message) => {
      clearTimeout(timer);
      worker.terminate();
      message.error ? reject(new Error(message.error)) : resolve(message.ok);
    });
    worker.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
  });

  for (const line of outcome) console.log(line);
  return 0;
}

// Worker threads import this file, and the isMainThread check keeps them
// out of the CLI. Set exitCode and do not call process.exit, because
// exiting before the event loop flushes stdout truncates results larger
// than the 64 KiB pipe buffer.
if (isMainThread && process.argv[1] && import.meta.url === new URL(`file://${process.argv[1]}`).href) {
  main(process.argv.slice(2))
    .then((code) => {
      process.exitCode = code;
    })
    .catch((error) => {
      console.error(error.message);
      process.exitCode = 1;
    });
}
