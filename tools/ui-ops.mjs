// UI OPS: a generated UI's `ui.json` lists every op its host asks
// (docs/architecture/generated-uis.md). gen-assemblies links a UI's
// assembly from that list. A host that asks an op of a capability the
// list does not name gets no module: its frame turns off at runtime,
// with no error. This gate finds that at test time.
//
// The check:
//
// - Every `ask("cap", "op", …)` in the host names an op that `ui.json`
//   lists, unless the op is a kernel op (every module answers those).
//   The list is `ops` (linked into the UI's assembly) and
//   `fetched_ops`: the ops of a capability the page fetches alone, only
//   when the deployment serves it. gen-assemblies does not link those.
// - Every capability the host names in `available`, `load`, or
//   `together` has at least one op in either list.
// - Every `ask(` in the host names its capability and op as string
//   literals, so this check can read it.
//
// `checkUi(...)` is the check (its broken fixtures: ui-ops.test.mjs).
// Run as a script, it checks each web/src/uis/<id>/ against the
// registry's kernel ops (`just test`).

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ASK = /(?<![\w.$])ask\(/g;
const LITERAL_ASK = /^ask\(\s*"([a-z_]+)"\s*,\s*"([a-z_]+)"/;
const CAP_CALL = /(?<![\w.$])(available|load|together)\(([^)]*)\)/g;

/// The problems in one UI: `source` is its host code, `ops` the
/// `ui.json` lists ("cap.op": `ops`, and `fetchedOps` from
/// `fetched_ops`), `kernelOps` the op names every module answers. An
/// empty list means the UI passes.
export function checkUi({ id, source, ops, fetchedOps = [], kernelOps }) {
  const listed = new Set([...ops, ...fetchedOps]);
  const linked = new Set([...ops, ...fetchedOps].map((o) => o.split(".")[0]));
  const kernel = new Set(kernelOps);
  const problems = [];
  for (const match of source.matchAll(ASK)) {
    const call = source.slice(match.index, match.index + 200);
    const literal = LITERAL_ASK.exec(call);
    const line = source.slice(0, match.index).split("\n").length;
    if (!literal) {
      problems.push(`${id}: line ${line} asks with a capability or op that is not a string literal`);
      continue;
    }
    const [, cap, op] = literal;
    if (!kernel.has(op) && !listed.has(`${cap}.${op}`)) {
      problems.push(`${id}: line ${line} asks ${cap}.${op}, which ui.json's ops do not list`);
    }
  }
  for (const match of source.matchAll(CAP_CALL)) {
    const line = source.slice(0, match.index).split("\n").length;
    for (const [, cap] of match[2].matchAll(/"([a-z_]+)"/g)) {
      if (!linked.has(cap)) {
        problems.push(`${id}: line ${line} names capability ${cap}, and ui.json lists none of its ops`);
      }
    }
  }
  return problems;
}

/// The host code of one UI: every .ts file in its directory.
function hostSource(dir) {
  return readdirSync(dir)
    .filter((f) => f.endsWith(".ts"))
    .sort()
    .map((f) => readFileSync(join(dir, f), "utf8"))
    .join("\n");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const registry = JSON.parse(readFileSync(join(root, "registry", "registry.json"), "utf8"));
  const kernelOps = (registry.kernel_ops ?? []).map((k) => k.name);
  if (kernelOps.length === 0) {
    console.error("UI OPS: the registry lists no kernel ops; run `just caps` first");
    process.exit(1);
  }
  const uis = join(root, "web", "src", "uis");
  const problems = [];
  for (const entry of readdirSync(uis, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const dir = join(uis, entry.name);
    const descriptor = join(dir, "ui.json");
    if (!existsSync(descriptor)) continue;
    const { ops = [], fetched_ops: fetchedOps = [] } = JSON.parse(readFileSync(descriptor, "utf8"));
    problems.push(...checkUi({ id: entry.name, source: hostSource(dir), ops, fetchedOps, kernelOps }));
  }
  if (problems.length) {
    for (const p of problems) console.error(`UI OPS FAIL: ${p}`);
    process.exit(1);
  }
}
