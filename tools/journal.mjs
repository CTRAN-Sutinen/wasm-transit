// The journal records what a session asked of its modules, as hashes.
// Replay proves that the same modules answer it again
// (docs/architecture/registry.md, "The journal").
//
// A journal lives in a session's directory in the runner's session
// store (`.caps-sessions/NAME/`):
//
// - `journal.jsonl` has one line for each module call:
//   {run, instance, cap, module, call, op, request, output}
//   - `run` is one command of the runner, or one page load.
//   - `instance` is the module instance that answered. Replay gives each
//     (instance, capability) pair a fresh single-capability instance.
//   - `cap` is the capability asked. `module` is the hash of the module
//     bytes that answered (a page's linked assembly, or one capability).
//   - `call` is "json" for a JSON op, else the bulk op's code.
//     `op` is the op's name, for the reader.
//   - `request` and `output` are the sha256 of the request bytes and of
//     the outcome text.
// - `blobs/<hex>.gz` holds each request's bytes once, gzipped, by
//   their hash. A journal line holds no document, only hashes.

import { createHash } from "node:crypto";
import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { gunzipSync, gzipSync } from "node:zlib";
import { join } from "node:path";

// A module refuses a document that it does not hold with the unknown_document
// code and its kind, hash, and load op (docs/architecture/bus.md, "Answers").
export function unknownDocument(outcome) {
  if (!outcome.startsWith('{"error"')) return null;
  try {
    const error = JSON.parse(outcome).error;
    return error?.code === "unknown_document" ? error : null;
  } catch {
    return null;
  }
}

export function sha256(bytes) {
  return "sha256:" + createHash("sha256").update(bytes).digest("hex");
}

const blobFile = (dir, hash) => join(dir, "blobs", `${hash.slice("sha256:".length)}.gz`);

export function journalWriter(dir, run) {
  const lines = [];
  const blobs = new Map();
  return {
    record({ instance, cap, module, call, op, payload, outcome }) {
      if (unknownDocument(outcome)) return;
      const request = sha256(payload);
      blobs.set(request, payload);
      lines.push(JSON.stringify({ run, instance, cap, module, call, op, request, output: sha256(outcome) }));
    },
    flush() {
      if (lines.length === 0) return;
      mkdirSync(join(dir, "blobs"), { recursive: true });
      for (const [hash, bytes] of blobs) {
        const file = blobFile(dir, hash);
        if (!existsSync(file)) writeFileSync(file, gzipSync(bytes));
      }
      // Blobs first, so a line never names a blob that is not there.
      appendFileSync(join(dir, "journal.jsonl"), lines.join("\n") + "\n");
    },
  };
}

/// The engine's `readJournal()` returns
/// `{journal: "<the lines>", blobs: {"sha256:…": "<base64 bytes>"}}`.
export function unpackJournal(dir, bundle) {
  mkdirSync(join(dir, "blobs"), { recursive: true });
  for (const [hash, base64] of Object.entries(bundle.blobs ?? {})) {
    const bytes = Buffer.from(base64, "base64");
    if (sha256(bytes) !== hash) throw new Error(`journal blob ${hash} does not hash to its name`);
    writeFileSync(blobFile(dir, hash), gzipSync(bytes));
  }
  writeFileSync(join(dir, "journal.jsonl"), bundle.journal ?? "");
}

export function readJournal(dir) {
  const file = join(dir, "journal.jsonl");
  if (!existsSync(file)) return [];
  return readFileSync(file, "utf8")
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function carried(ok) {
  if (!ok || typeof ok !== "object") return [];
  return Object.entries(ok).filter(
    ([field, value]) =>
      value !== null &&
      typeof value === "object" &&
      typeof value.kind === "string" &&
      typeof value.schema_version === "number" &&
      typeof ok[`${field}_hash`] === "string",
  );
}

/// Replay semantics are in docs/architecture/registry.md, "The journal".
///
/// - `place(cap, module)` returns `{linked, open()}`, and `open()` returns
///   `{module, invokeRaw(bytes) -> text, invokeBytes(code, bytes) -> text}`.
///   A `linked: false` instance loses the `cap` field of a request and the
///   capability index in the high 16 bits of a bulk op.
/// - `stateOps` maps `evict` and `held` to their codes. Replay compares
///   their answers only on the recorded module, because an instance split
///   from a page's assembly holds less. It counts the rest as `unchecked`.
// Each call with its request: the JSON a JSON request or a load held, or
// the size of a bulk request's other bytes (a feed, an rt snapshot). A
// page's linked assembly is asked with a `cap` field, which the entry
// already names, so it is dropped.
export function calls(dir) {
  return readJournal(dir).map((entry, index) => {
    const file = blobFile(dir, entry.request);
    const call = { index, cap: entry.cap, op: entry.op };
    if (!existsSync(file)) return { ...call, request: null };
    const bytes = gunzipSync(readFileSync(file));
    try {
      const request = JSON.parse(bytes.toString("utf8"));
      if (request && typeof request === "object" && !Array.isArray(request)) delete request.cap;
      return { ...call, request };
    } catch {
      return { ...call, request: { bytes: bytes.length } };
    }
  });
}

export function replay({ dir, place, kernelLoads, stateOps = {} }) {
  const entries = readJournal(dir);
  const instances = new Map();
  const documents = new Map();
  const loadCodes = new Set(Object.values(kernelLoads));
  const report = { entries: entries.length, same: 0, unchecked: 0, differ: [], cannot: [], modules: {} };
  const stateNames = new Set(Object.keys(stateOps));
  const stateCodes = new Set(Object.values(stateOps));

  entries.forEach((entry, index) => {
    const where = { index, run: entry.run, cap: entry.cap, op: entry.op };
    const file = blobFile(dir, entry.request);
    if (!existsSync(file)) {
      report.cannot.push({ ...where, why: "the request's bytes are not in the store" });
      return;
    }
    let payload = gunzipSync(readFileSync(file));
    const placed = place(entry.cap, entry.module);
    const key = `${entry.instance}\n${placed.linked ? "" : entry.cap}`;
    if (!instances.has(key)) instances.set(key, placed.open());
    const instance = instances.get(key);
    const modules = (report.modules[entry.cap] ??= { recorded: [], replayed: [] });
    if (!modules.recorded.includes(entry.module)) modules.recorded.push(entry.module);
    if (!modules.replayed.includes(instance.module)) modules.replayed.push(instance.module);

    let code = entry.call;
    if (!placed.linked) {
      if (entry.call === "json") {
        const request = JSON.parse(payload.toString("utf8"));
        if ("cap" in request) {
          delete request.cap;
          payload = Buffer.from(JSON.stringify(request));
        }
      } else {
        code = entry.call & 0xffff;
      }
    }
    const ask = () => (code === "json" ? instance.invokeRaw(payload) : instance.invokeBytes(code, payload));
    let text = ask();
    for (let tries = 0; tries < 8; tries++) {
      const missing = unknownDocument(text);
      const source = missing && documents.get(missing.hash);
      if (!source) break;
      const loaded = source.call === "json" ? instance.invokeRaw(source.payload) : instance.invokeBytes(source.call, source.payload);
      if (!loaded.includes(missing.hash)) break;
      text = ask();
    }
    const missing = unknownDocument(text);
    if (missing) {
      report.cannot.push({ ...where, why: `${missing.kind} ${missing.hash} never crossed to the journal's host` });
      return;
    }
    const aboutInstance = code === "json" ? stateNames.has(entry.op) : stateCodes.has(code & 0xffff);
    if (aboutInstance && instance.module !== entry.module) report.unchecked += 1;
    else if (sha256(text) === entry.output) report.same += 1;
    else report.differ.push({ ...where, answered: text.length > 300 ? `${text.slice(0, 297)}…` : text });

    // The documents that this call registered or created, for the instances after it.
    const isLoad = code === "json" ? kernelLoads[entry.op] !== undefined : loadCodes.has(code & 0xffff);
    if (!text.includes('_hash"')) return;
    const ok = JSON.parse(text).ok;
    if (!ok || typeof ok !== "object") return;
    if (isLoad) {
      for (const [field, value] of Object.entries(ok)) {
        if (field.endsWith("_hash") && typeof value === "string" && !documents.has(value)) {
          documents.set(value, { call: code === "json" ? code : code & 0xffff, payload });
        }
      }
    }
    for (const [field, value] of carried(ok)) {
      const load = kernelLoads[`load_${value.kind}`];
      if (load !== undefined && !documents.has(ok[`${field}_hash`])) {
        documents.set(ok[`${field}_hash`], { call: load, payload: Buffer.from(JSON.stringify(value)) });
      }
    }
  });
  return report;
}
