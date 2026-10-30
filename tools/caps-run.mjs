// caps-run — the agent (and human) runner for the capability
// registry: pull a module by its pinned hash, VERIFY the bytes,
// execute ops in the sandbox (zero imports), with a wall-clock
// guard via worker termination.
//
// This is the dogfood interface: a human answers the same questions
// an agent can, through the same calls — the registry record's
// op_docs are the tool descriptions, the worked examples are the
// few-shot, and the output hashes chain ops exactly the way an
// agent's context does.
//
//   caps-run list                       capabilities + evidence
//   caps-run tools <capability>         the tool catalog (op docs, and
//                                       per-kind examples where an op
//                                       takes a tagged vocabulary)
//   caps-run <capability> [--cap ID] [--as NAME]
//          [--load FILE]... [--op '<json>|@FILE']... [--ask URL]...
//          [--timeout MS]               run a session
//
// Every run compares the registry's recorded source digest with the
// tree's and says on stderr when they differ — the registry runs the
// code it was built from, not the code in the tree (`just caps`).
//
// --load [N:]FILE sends the file's bytes on bulk op N (default 0 —
// feeds, layers; a module with a second bulk op spells it, e.g.
// cap-rt's `--load 1:feed.zip` loads the static feed for joins;
// `--load 256:network.json` registers a network document on the
// kernel op any network consumer answers).
// --op invokes one JSON op; repeatable; '@FILE' reads the JSON from
// a file. Steps run in COMMAND-LINE ORDER, loads and ops alike;
// results print as JSON, one per line, in that order.
//
// --ask URL asks an ORACLE — an external service whose answer is an
// input the user could have given by hand (docs/ARCHITECTURE.md, web
// section, "External services") — through the deployment's path, the
// way a page asks it: `--ask 'http://127.0.0.1:8080/geocode/
// autocomplete?text=peacehealth'`. Its JSON answer joins the session
// as a result (`$ask.features.0.geometry.coordinates.1`), so "the
// rider typed X and picked the first" chains into the ops.
//
// A session spans capabilities: --cap ID switches the capability
// the following steps run against (each module is verified and
// instantiated once per session). Documents cross by reference —
// `"network":"$import.network"` hands the imported document to the
// next capability, the same bytes, no file in between; assert the
// identities agree like any host does.
//
// Result references: a string value that starts with "$" resolves
// against the session's PRIOR results before invoking — no
// hand-copying hashes between ops:
//   $net.network_hash  the result named by `--as net` (names are the
//                      stable address — they survive reordering)
//   $load.feed_hash    the last --load result ("load" op)
//   $ask.features      the last --ask result (the oracle's answer)
//   $import.network_hash  the last prior result tagged op:"import"
//   $1.feed_hash       the 1st result overall, in command-line order
// The rest of the reference is a dotted path into the result
// (numeric segments index arrays). A reference must be the whole
// string value; unresolvable references fail the session loudly.
//
// The clock (the HOST owns it — dates never cross as magic):
//   $TODAY             the host's local date, "YYYY-MM-DD"
//   $NOW_SECS          the host's local seconds since midnight (a
//                      number — the seconds fields are u32)

import { createHash } from "node:crypto";
import { parentPort, workerData, isMainThread } from "node:worker_threads";
import { existsSync, readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REGISTRY = join(HERE, "..", "registry");

function registryPointer() {
  return JSON.parse(readFileSync(join(REGISTRY, "registry.json"), "utf8"));
}

// The digest of the source the registry's modules are built from,
// computed exactly as gen-manifest records it (`source_digest`): every
// file git sees under crates/ plus Cargo.toml and Cargo.lock, sha256
// over "{path}\n{sha256 hex}\n" in path order. Null without git.
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

// A registry published from another tree runs other code than the
// tree says: say so on every run (QAF-77), never silently.
function warnIfStale() {
  const published = registryPointer().evidence?.source_digest ?? null;
  const tree = sourceDigest();
  if (!published || !tree || published === tree) return;
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
    "usage: caps-run list | caps-run tools <capability> | " +
      "caps-run <capability> [--cap ID] [--as NAME] [--load FILE]... [--op JSON|@FILE]... [--ask URL]... [--timeout MS]",
  );
  process.exit(2);
}

function parseArgs(argv) {
  // Steps keep command-line order: each names the capability it runs
  // against (the positional one until a --cap switches it) and the
  // name a following --as gave it.
  const out = { capability: null, steps: [], timeout: 10_000 };
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
    else if (arg === "--cap") current = argv[++i] ?? usage();
    else if (arg === "--as") pendingName = argv[++i] ?? usage();
    else if (arg === "--timeout") out.timeout = Number(argv[++i]);
    else if (arg.startsWith("--")) usage();
    else if (out.capability === null) out.capability = current = arg;
    else usage();
  }
  if (!out.capability || pendingName !== null) usage();
  return out;
}

// -----------------------------------------------------------------
// The sandbox session (worker thread: the parent can TERMINATE a
// runaway computation — the wall-clock half of metering; the
// zero-import wasm sandbox is the other half).
// -----------------------------------------------------------------

// One verified, instantiated capability module: the bytes are checked
// against the registry record's pin before anything runs.
function instantiate(capability) {
  const record = capabilityRecord(capability);
  const bytes = readFileSync(join(REGISTRY, record.blob));
  const hash = sha256(bytes);
  if (hash !== record.hash) {
    throw new Error(`registry integrity: blob is ${hash}, record pins ${record.hash}`);
  }
  const mod = new WebAssembly.Module(bytes);
  const imports = WebAssembly.Module.imports(mod);
  if (imports.length !== 0) throw new Error(`capability ${capability} has imports — refusing`);
  const ex = new WebAssembly.Instance(mod, {}).exports;

  // The outcome stays the module's JSON TEXT: a session holds every
  // result for later references, and one string per result is all the
  // collector marks — a parsed real-size answer is millions of
  // objects, re-marked by every major collection while the next large
  // answer runs (QAF-86).
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
  return {
    invokeBytes: (op, payload) => call(payload, (ptr, len) => ex.transit_invoke_bytes(op, ptr, len)),
    invoke: (request) => call(Buffer.from(JSON.stringify(request)), (ptr, len) => ex.transit_invoke(ptr, len)),
  };
}

async function runSession() {
  const { steps } = workerData;
  const modules = new Map();
  const moduleFor = (capability) => {
    if (!modules.has(capability)) modules.set(capability, instantiate(capability));
    return modules.get(capability);
  };

  // Each result: its metadata (`as`, `cap`, `load`, `ask`, the op its
  // answer is tagged with) and its outcome as JSON text, parsed only
  // when a reference reaches into it.
  const results = [];
  const outcomeOf = (r) => JSON.parse(r.text);
  // The op an answer is tagged with, read the first time a `$op`
  // reference asks — only the tag is kept.
  const opOf = (r) => {
    if (r.op === undefined) r.op = outcomeOf(r).ok?.op ?? null;
    return r.op;
  };
  // The clock: the HOST owns it, so $TODAY / $NOW_SECS resolve to the
  // host's here-and-now before the ops ever see them. Result
  // references ($load.x, $import.x, $1.x) resolve against the
  // session's prior results — both BEFORE the bytes cross the ABI,
  // so the agent never hand-copies a hash or computes a date.
  const now = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  const clockRefs = {
    $TODAY: `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`,
    $NOW_SECS: now.getHours() * 3600 + now.getMinutes() * 60 + now.getSeconds(),
  };
  const resolveRef = (ref) => {
    const m = /^\$(load|ask|\d+|[a-zA-Z_][a-zA-Z0-9_]*)(?:\.([a-zA-Z0-9_.]*))?$/.exec(ref);
    if (!m) throw new Error(`bad result reference ${JSON.stringify(ref)}: expected $<name|load|ask|op|N>.<path>`);
    const [, base, path] = m;
    let current;
    const named = results.find((r) => r.as === base);
    if (named) {
      current = named;
    } else if (base === "load") {
      current = [...results].reverse().find((r) => r.load !== undefined);
      if (!current) throw new Error(`reference ${ref}: no --load result to resolve against`);
    } else if (base === "ask") {
      current = [...results].reverse().find((r) => r.ask !== undefined);
      if (!current) throw new Error(`reference ${ref}: no --ask result to resolve against`);
    } else if (/^\d+$/.test(base)) {
      current = results[Number(base) - 1];
      if (!current) throw new Error(`reference ${ref}: no result ${base} (the session has ${results.length})`);
    } else {
      current = [...results].reverse().find((r) => opOf(r) === base);
      if (!current) throw new Error(`reference ${ref}: no prior result tagged op:"${base}"`);
    }
    // References resolve into the result's ok payload — the document
    // the session prints and the agent reads (the outcome wrapper is
    // transport, not content).
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
      if (clockRefs[value] !== undefined) return clockRefs[value];
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
    // The printed line's leading fields, in this order: the name, the
    // capability, then the step's own (load, ask), then the outcome.
    const result = {};
    if (step.name) {
      if (results.some((r) => r.as === step.name)) throw new Error(`--as ${step.name}: the name is already taken`);
      result.as = step.name;
    }
    // The capability rides on every line once a session spans more
    // than one — the reader sees which module answered.
    if (new Set(steps.map((s) => s.capability)).size > 1) result.cap = step.capability;
    if (step.kind === "ask") {
      // The oracle answers over HTTP, outside the sandbox — its
      // answer is input, like a file the session loads.
      const resp = await fetch(step.value);
      result.ask = step.value;
      result.text = resp.ok ? `{"ok":${JSON.stringify(await resp.json())}}` : JSON.stringify({ error: `HTTP ${resp.status}` });
    } else if (step.kind === "load") {
      const module = moduleFor(step.capability);
      const match = /^(\d+):(.+)$/.exec(step.value);
      const op = match ? Number(match[1]) : 0;
      const file = match ? match[2] : step.value;
      result.load = file;
      result.text = module.invokeBytes(op, readFileSync(file));
    } else {
      const module = moduleFor(step.capability);
      const request = step.value.startsWith("@")
        ? JSON.parse(readFileSync(step.value.slice(1), "utf8"))
        : JSON.parse(step.value);
      result.text = module.invoke(resolveRefs(request));
    }
    results.push(result);
  }
  // One printed line per result: the leading fields, then the
  // outcome's own fields — spliced as text, never re-parsed.
  const lines = results.map(({ text, op, ...lead }) => {
    const head = JSON.stringify(lead);
    return head === "{}" ? text : `${head.slice(0, -1)},${text.slice(1)}`;
  });
  parentPort.postMessage({ ok: lines });
}

if (!isMainThread) {
  runSession().catch((error) => {
    parentPort.postMessage({ error: String(error) });
  });
}

// -----------------------------------------------------------------
// The parent: argument parsing, listing, catalogs, and the guard.
// -----------------------------------------------------------------

export async function main(argv) {
  warnIfStale();
  if (argv[0] === "list") {
    const pointer = registryPointer();
    console.log(JSON.stringify(pointer, null, 2));
    return 0;
  }
  if (argv[0] === "tools") {
    const record = capabilityRecord(argv[1] ?? usage());
    const tools = Object.fromEntries(
      record.ops.map((op) => [op, record.op_docs[op] ?? { description: null, example: null }]),
    );
    console.log(JSON.stringify({ id: record.id, purpose: record.purpose, tools }, null, 2));
    return 0;
  }

  const { steps, timeout } = parseArgs(argv);
  // Unknown capabilities fail before the sandbox spins up.
  for (const capability of new Set(steps.map((s) => s.capability))) capabilityRecord(capability);
  const { Worker } = await import("node:worker_threads");
  const worker = new Worker(new URL(import.meta.url), {
    workerData: { steps },
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

// Entry: run as a CLI only when invoked as main module (worker
// threads import this same file — the isMainThread branch above
// keeps them out of here). exitCode (NOT process.exit): exiting
// without the event loop flushing stdout TRUNCATES results larger
// than the 64 KiB pipe buffer — found by dogfooding the C-TRAN
// feed through this exact CLI.
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
