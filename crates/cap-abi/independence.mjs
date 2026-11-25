// Assembly independence (D-77, docs/architecture/bus.md, "Bus rules").
// Every manifest worked example must answer the same from a capability
// built alone and from inside each generated UI's linked assembly, in
// either order of the assembly's capabilities. A derived entry that one
// capability computes and another finds is a cache, never a dependency.
// Run as a script, this checks the built modules in web/public/caps, and
// the server's in web/pkg/server.wasm.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const OP_LOAD_FEED = 266;
const OP_LOAD_RT = 267;
const OP_ABOUT = 268;

function setup(manifest, fixtures) {
  const steps = [];
  for (const op of manifest.kernel_ops ?? []) {
    if (op.name.startsWith("load_") && op.example?.request) steps.push({ kind: op.name.slice(5), json: op.example.request });
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

// `instantiate(entry)` returns a fresh instance of a capability's module or
// of a linked assembly.
export function independence({ manifest, fixtures = {}, instantiate }) {
  const byId = Object.fromEntries(manifest.capabilities.map((c) => [c.id, c]));
  const steps = setup(manifest, fixtures);
  const failures = [];
  let compared = 0;
  let answered = 0;
  for (const asm of manifest.assemblies ?? []) {
    const answeredBefore = answered;
    // One instance per capability, as a runner holds it.
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
    // Index order, then reverse in a second instance: what one capability
    // computed first must not change what another answers.
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

export function shell(bytes) {
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
    if (!modules.has(entry.url)) modules.set(entry.url, readFileSync(entry.file ?? path.join(dist, "..", entry.url)));
    return shell(modules.get(entry.url));
  };
  // The server's assembly (D-227) is checked as a UI's is. Its own about
  // op names its capabilities, in index order.
  const server = path.join(here, "../../web/pkg/server.wasm");
  const about = JSON.parse(shell(readFileSync(server)).invokeBytes(OP_ABOUT, Buffer.alloc(0)));
  manifest.assemblies = [
    ...(manifest.assemblies ?? []),
    { ui: "server", url: "server.wasm", file: server, capabilities: about.ok.capabilities.map((c) => c.id) },
  ];
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
