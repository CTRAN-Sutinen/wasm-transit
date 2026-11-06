// ASSEMBLY INDEPENDENCE (D-77, docs/ARCHITECTURE.md "Bus rules"): every
// manifest worked example answers identically from the capability
// built alone and from inside each generated UI's linked assembly — in
// either order of the assembly's capabilities. A derived entry one
// capability computed and another found is only a cache, never a
// dependency: if a capability's answer changed with what its
// neighbours computed first, this gate fails.
//
// `independence(...)` is the check (its broken fixture:
// independence.test.mjs); run as a script, it checks the built modules
// in web/public/caps (`just test`).

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const OP_LOAD_FEED = 266;
const OP_LOAD_RT = 267;

/// The documents every instance registers before the examples run: the
/// kernel's worked load examples (JSON) and the bulk fixtures, each
/// sent only where a linked capability reads its kind.
function setup(manifest, fixtures) {
  const kinds = {
    load_network: "network",
    load_edits: "edits",
    load_live: "live",
    load_boundary: "boundary",
    load_walks: "walks",
    load_streets: "streets",
    load_watchlist: "watchlist",
    load_work_rules: "work_rules",
  };
  const steps = [];
  for (const op of manifest.kernel_ops ?? []) {
    if (kinds[op.name] && op.example?.request) steps.push({ kind: kinds[op.name], json: op.example.request });
  }
  if (fixtures.feed) steps.push({ kind: "feed", op: OP_LOAD_FEED, bytes: fixtures.feed });
  if (fixtures.rt) steps.push({ kind: "rt", op: OP_LOAD_RT, bytes: fixtures.rt });
  return steps;
}

function register(instance, steps, reads) {
  for (const step of steps) {
    if (!reads.has(step.kind)) continue;
    if (step.json) instance.invoke(step.json);
    else instance.invokeBytes(step.op, step.bytes);
  }
}

/// The check. `instantiate(entry)` makes a fresh instance of a
/// capability's module or of a linked assembly: `{invoke(request) ->
/// outcome text, invokeBytes(op, bytes) -> outcome text}`.
export function independence({ manifest, fixtures = {}, instantiate }) {
  const byId = Object.fromEntries(manifest.capabilities.map((c) => [c.id, c]));
  const steps = setup(manifest, fixtures);
  const failures = [];
  let compared = 0;
  let answered = 0;
  for (const asm of manifest.assemblies ?? []) {
    const answeredBefore = answered;
    // The answers alone: one instance per capability, as a runner holds it.
    const alone = new Map();
    for (const id of asm.capabilities) {
      const cap = byId[id];
      const instance = instantiate(cap);
      register(instance, steps, new Set(cap.documents ?? []));
      const answers = new Map();
      for (const [op, doc] of Object.entries(cap.op_docs ?? {})) {
        if (doc.example?.request) answers.set(op, instance.invoke(doc.example.request));
      }
      alone.set(id, answers);
    }
    // The answers linked: the capabilities in index order, then again
    // in reverse in a second instance — what one computed first must
    // never change what another answers.
    const reads = new Set(asm.capabilities.flatMap((id) => byId[id].documents ?? []));
    for (const order of [asm.capabilities, [...asm.capabilities].reverse()]) {
      const linked = instantiate(asm);
      register(linked, steps, reads);
      for (const id of order) {
        for (const [op, want] of alone.get(id)) {
          const request = { cap: id, ...byId[id].op_docs[op].example.request };
          const got = linked.invoke(request);
          compared += 1;
          if (got.startsWith('{"ok"')) answered += 1;
          if (got !== want) {
            failures.push(`${asm.ui}: ${id}.${op} answers differently linked (${order[0]} first) than alone`);
          }
        }
      }
    }
    if (answered === answeredBefore) failures.push(`${asm.ui}: no worked example answered — the gate compared nothing`);
  }
  return { compared, answered, failures };
}

// ---------------------------------------------------------------------
// the gate over the built modules
// ---------------------------------------------------------------------

function shell(bytes) {
  const ex = new WebAssembly.Instance(new WebAssembly.Module(bytes), {}).exports;
  const call = (payload, enter) => {
    const ptr = ex.transit_alloc(payload.length);
    new Uint8Array(ex.memory.buffer, ptr, payload.length).set(payload);
    const out = enter(ptr, payload.length);
    ex.transit_free(ptr, payload.length);
    const len = new DataView(ex.memory.buffer).getUint32(out, true);
    const text = Buffer.from(ex.memory.buffer, out + 4, len).toString("utf8");
    ex.transit_free(out, len + 4);
    return text;
  };
  return {
    invoke: (request) => call(Buffer.from(JSON.stringify(request)), (p, l) => ex.transit_invoke(p, l)),
    invokeBytes: (op, payload) => call(payload, (p, l) => ex.transit_invoke_bytes(op, p, l)),
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const here = path.dirname(fileURLToPath(import.meta.url));
  const dist = path.join(here, "../../web/public/caps");
  const manifest = JSON.parse(readFileSync(path.join(dist, "manifest.json"), "utf8"));
  const modules = new Map();
  const instantiate = (entry) => {
    if (!modules.has(entry.url)) modules.set(entry.url, readFileSync(path.join(dist, "..", entry.url)));
    return shell(modules.get(entry.url));
  };
  const fixtures = {
    feed: readFileSync(path.join(dist, "sample-gtfs.zip")),
    rt: readFileSync(path.join(here, "../../web/e2e/fixtures/sample-rt.pb")),
  };
  const { compared, answered, failures } = independence({ manifest, fixtures, instantiate });
  if (failures.length) {
    for (const f of failures) console.error("INDEPENDENCE FAIL: " + f);
    process.exit(1);
  }
  console.log(`assembly independence: ${compared} worked examples alike linked and alone (${answered} answered ok)`);
}
