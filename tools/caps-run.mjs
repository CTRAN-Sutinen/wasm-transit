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
//   caps-run tools <capability>         the tool catalog (op docs)
//   caps-run <capability> [--load FILE]... [--op '<json>|@FILE']...
//          [--timeout MS]               run a session
//
// --load [N:]FILE sends the file's bytes on bulk op N (default 0 —
// feeds, layers; a module with a second bulk op spells it, e.g.
// cap-rt's `--load 1:feed.zip` loads the static feed for joins).
// --op invokes one JSON op; repeatable; '@FILE' reads the JSON from
// a file. Results print as JSON, one per line, in order.
//
// Result references: a string value that starts with "$" resolves
// against the session's PRIOR results before invoking — no
// hand-copying hashes between ops:
//   $load.feed_hash    the last --load result ("load" op)
//   $import.network_hash  the last prior result tagged op:"import"
//   $1.feed_hash       the 1st result overall (loads included)
// The rest of the reference is a dotted path into the result
// (numeric segments index arrays). A reference must be the whole
// string value; unresolvable references fail the session loudly.
//
// The clock (the HOST owns it — dates never cross as magic):
//   $TODAY             the host's local date, "YYYY-MM-DD"
//   $NOW_SECS          the host's local seconds since midnight

import { createHash } from "node:crypto";
import { parentPort, workerData, isMainThread } from "node:worker_threads";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REGISTRY = join(HERE, "..", "registry");

function registryPointer() {
  return JSON.parse(readFileSync(join(REGISTRY, "registry.json"), "utf8"));
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
      "caps-run <capability> [--load FILE]... [--op JSON|@FILE]... [--timeout MS]",
  );
  process.exit(2);
}

function parseArgs(argv) {
  const out = { capability: null, loads: [], ops: [], timeout: 10_000 };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--load") out.loads.push(argv[++i]);
    else if (arg === "--op") out.ops.push(argv[++i]);
    else if (arg === "--timeout") out.timeout = Number(argv[++i]);
    else if (arg.startsWith("--")) usage();
    else if (out.capability === null) out.capability = arg;
    else usage();
  }
  if (!out.capability) usage();
  return out;
}

// -----------------------------------------------------------------
// The sandbox session (worker thread: the parent can TERMINATE a
// runaway computation — the wall-clock half of metering; the
// zero-import wasm sandbox is the other half).
// -----------------------------------------------------------------

async function runSession() {
  const { capability, loads, ops } = workerData;
  const record = capabilityRecord(capability);
  const bytes = readFileSync(join(REGISTRY, record.blob));
  const hash = sha256(bytes);
  if (hash !== record.hash) {
    throw new Error(`registry integrity: blob is ${hash}, record pins ${record.hash}`);
  }
  const mod = new WebAssembly.Module(bytes);
  const imports = WebAssembly.Module.imports(mod);
  if (imports.length !== 0) throw new Error(`capability ${capability} has imports — refusing`);
  const inst = new WebAssembly.Instance(mod, {});
  const ex = inst.exports;

  const read = (outPtr) => {
    const len = new DataView(ex.memory.buffer).getUint32(outPtr, true);
    const outcome = JSON.parse(Buffer.from(ex.memory.buffer, outPtr + 4, len).toString("utf8"));
    ex.transit_free(outPtr, len + 4);
    return outcome;
  };
  const invokeBytes = (op, payload) => {
    const ptr = ex.transit_alloc(payload.length);
    new Uint8Array(ex.memory.buffer, ptr, payload.length).set(payload);
    const outPtr = ex.transit_invoke_bytes(op, ptr, payload.length);
    ex.transit_free(ptr, payload.length);
    return read(outPtr);
  };
  const invoke = (request) => {
    const payload = Buffer.from(JSON.stringify(request));
    const ptr = ex.transit_alloc(payload.length);
    new Uint8Array(ex.memory.buffer, ptr, payload.length).set(payload);
    const outPtr = ex.transit_invoke(ptr, payload.length);
    ex.transit_free(ptr, payload.length);
    return read(outPtr);
  };

  const results = [];
  for (const load of loads) {
    const match = /^(\d+):(.+)$/.exec(load);
    const op = match ? Number(match[1]) : 0;
    const file = match ? match[2] : load;
    const payload = readFileSync(file);
    results.push({ load: file, ...invokeBytes(op, payload) });
  }
  // The clock: the HOST owns it, so $TODAY / $NOW_SECS resolve to the
  // host's here-and-now before the ops ever see them. Result
  // references ($load.x, $import.x, $1.x) resolve against the
  // session's prior results — both BEFORE the bytes cross the ABI,
  // so the agent never hand-copies a hash or computes a date.
  const now = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  const clockRefs = {
    $TODAY: `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`,
    $NOW_SECS: String(now.getHours() * 3600 + now.getMinutes() * 60 + now.getSeconds()),
  };
  const resolveRef = (ref) => {
    const m = /^\$(load|\d+|[a-z_][a-z0-9_]*)(?:\.([a-zA-Z0-9_.]*))?$/.exec(ref);
    if (!m) throw new Error(`bad result reference ${JSON.stringify(ref)}: expected $<load|op|N>.<path>`);
    const [, base, path] = m;
    let current;
    if (base === "load") {
      current = [...results].reverse().find((r) => r.load !== undefined);
      if (!current) throw new Error(`reference ${ref}: no --load result to resolve against`);
    } else if (/^\d+$/.test(base)) {
      current = results[Number(base) - 1];
      if (!current) throw new Error(`reference ${ref}: no result ${base} (the session has ${results.length})`);
    } else {
      current = [...results].reverse().find((r) => r.ok?.op === base);
      if (!current) throw new Error(`reference ${ref}: no prior result tagged op:"${base}"`);
    }
    // References resolve into the result's ok payload — the document
    // the session prints and the agent reads (the outcome wrapper is
    // transport, not content).
    current = current.ok ?? current;
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
  for (const op of ops) {
    const request = op.startsWith("@")
      ? JSON.parse(readFileSync(op.slice(1), "utf8"))
      : JSON.parse(op);
    results.push(invoke(resolveRefs(request)));
  }
  parentPort.postMessage({ ok: results });
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

  const { capability, loads, ops, timeout } = parseArgs(argv);
  const record = capabilityRecord(capability);
  const { Worker } = await import("node:worker_threads");
  const worker = new Worker(new URL(import.meta.url), {
    workerData: { capability, loads, ops },
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

  for (const result of outcome) console.log(JSON.stringify(result));
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
