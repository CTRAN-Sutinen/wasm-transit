// The journal: what a session asked of its modules, as hashes, and
// replay, the proof that the same modules answer it again
// (docs/architecture/registry.md, "The journal").
//
// A journal lives in a session's directory in the runner's session
// store (`.caps-sessions/NAME/`):
//
// - `journal.jsonl` has one line for each module call:
//   {run, instance, cap, module, call, op, request, output}
//   - `run`: one command of the runner, or one page load.
//   - `instance`: the module instance that answered. Replay gives each
//     (instance, capability) pair a fresh single-capability instance.
//   - `cap`: the capability asked. `module`: the hash of the module
//     bytes that answered (a page's linked assembly, or one capability).
//   - `call`: "json" for a JSON op, else the bulk op's code.
//     `op`: the op's name, for the reader.
//   - `request`, `output`: the sha256 of the request bytes and of the
//     outcome text.
// - `blobs/<hex>.gz` holds each request's bytes once, gzipped, by
//   their hash. A journal line holds no document, only hashes.
//
// A call that a module refuses because it does not hold a document
// ("unknown <kind> <hash>: <load> first") is not journaled. The host
// registers the document and asks again, and the journal records that
// load and the answer.

import { createHash } from "node:crypto";
import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { gunzipSync, gzipSync } from "node:zlib";
import { join } from "node:path";

// The kernel's label for the kind is free text ("live document"); the
// load op's name is exact.
export const UNKNOWN_DOCUMENT = /^\{"error":"unknown (.+?) (sha256:[0-9a-f]{64}): (load_[a-z_]+) first"\}$/;

export function sha256(bytes) {
  return "sha256:" + createHash("sha256").update(bytes).digest("hex");
}

const blobFile = (dir, hash) => join(dir, "blobs", `${hash.slice("sha256:".length)}.gz`);

/// A writer for one run: `record(...)` for each module call, then
/// `flush()` to append the run's lines and blobs to the session.
export function journalWriter(dir, run) {
  const lines = [];
  const blobs = new Map();
  return {
    record({ instance, cap, module, call, op, payload, outcome }) {
      if (UNKNOWN_DOCUMENT.test(outcome)) return;
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

/// A page's journal as one file (the engine's `readJournal()`):
/// `{journal: "<the lines>", blobs: {"sha256:…": "<base64 bytes>"}}`.
/// Unpacked, it is a session directory that replay reads.
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

// The documents an answer carries at its top level, each beside its
// hash (`network` and `network_hash`).
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

/// Replay a journal on fresh instances, in order, and compare each
/// output hash with the recorded one.
///
/// - `place(cap, module)` says where an entry of `cap`, recorded on the
///   module with hash `module`, runs again: `{linked, open()}`. `open()`
///   gives `{module, invokeRaw(bytes) -> text, invokeBytes(code, bytes)
///   -> text}`.
///   - `linked: true` is a linked assembly, normally the bytes the page
///     ran. One instance answers every capability of a journal instance,
///     and each request is sent as recorded.
///   - `linked: false` is one capability's own module, or a candidate
///     that replaces it. Each capability of a journal instance gets its
///     own instance. A request loses the `cap` field that names the
///     capability in a linked assembly, and a bulk op loses the
///     capability's index in its high 16 bits.
/// - `kernelLoads` maps a kernel load's name to its bulk op code.
/// - `stateOps` maps the kernel ops that answer about the instance
///   (`evict`, `held`) to their codes. Replay runs them, and compares
///   their answers only on the recorded module: an instance split from
///   a page's assembly holds less, and says so. It counts the rest as
///   `unchecked`.
///
/// A document that another instance registered or minted is registered
/// in the asking instance when it asks for it, as a linked page shares
/// it. The answer is the same by assembly independence. A document
/// that never crossed out of a linked assembly cannot cross to another
/// instance, and replay says so.
export function replay({ dir, place, kernelLoads, stateOps = {} }) {
  const entries = readJournal(dir);
  const instances = new Map();
  // Each document seen so far, by hash: how to register it again.
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
      const missing = UNKNOWN_DOCUMENT.exec(text);
      const source = missing && documents.get(missing[2]);
      if (!source) break;
      const loaded = source.call === "json" ? instance.invokeRaw(source.payload) : instance.invokeBytes(source.call, source.payload);
      if (!loaded.includes(missing[2])) break;
      text = ask();
    }
    const missing = UNKNOWN_DOCUMENT.exec(text);
    if (missing) {
      report.cannot.push({ ...where, why: `${missing[1]} ${missing[2]} never crossed to the journal's host` });
      return;
    }
    const aboutInstance = code === "json" ? stateNames.has(entry.op) : stateCodes.has(code & 0xffff);
    if (aboutInstance && instance.module !== entry.module) report.unchecked += 1;
    else if (sha256(text) === entry.output) report.same += 1;
    else report.differ.push({ ...where, answered: text.length > 300 ? `${text.slice(0, 297)}…` : text });

    // What this call registered or minted, for the instances after it.
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
