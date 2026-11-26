// Parity (docs/architecture/web.md, "Parity"): the browser runs the same
// ops as caps-run and the Go server, so web/ holds host glue and render
// components, never answers. Each rule names the files it reads, what
// breaks it, and the fix. parity.test.mjs is the broken fixture.

import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { files, matching, report } from "../../tools/gates/files.mjs";

const source = (root) => files(root, "web/src").filter((p) => /\.(ts|svelte)$/.test(p));
const notTypes = (p) => !p.startsWith("web/src/types/");
const hosts = (root) => source(root).filter((p) => /^web\/src\/uis\/[^/]+\/host\.svelte\.ts$/.test(p));

export const RULES = [
  {
    name: "bus-in-components",
    files: (root) => source(root).filter((p) => /^web\/src\/(components|uis\/[^/]+|picker)\/[^/]+\.svelte$/.test(p)),
    pattern: /from "[./]*(engine|bus)"/,
    fix: "a component imports the engine or the bus; take node values as props",
  },
  {
    name: "rt-join-in-web",
    files: (root) => source(root).filter(notTypes),
    pattern: /stop_updates/,
    fix: "reads raw rt stop_updates; the live join is cap-rt's",
  },
  {
    name: "clone-and-mutate",
    files: (root) => source(root).filter(notTypes),
    pattern: /structuredClone/,
    fix: "clones a document to mutate it; send NetworkEdits through cap-author",
  },
  {
    name: "bus-outside-engine",
    files: (root) => source(root).filter((p) => p !== "web/src/engine.ts" && p !== "web/src/bus.worker.ts"),
    pattern: /from "\.\/bus"|from "(\.\.\/)+bus"/,
    fix: "imports the bus; ask through the engine (src/engine.ts)",
  },
  {
    name: "worker-outside-bus",
    files: (root) => source(root).filter((p) => p !== "web/src/bus.ts"),
    pattern: /new Worker\(/,
    fix: "spawns the bus worker; ask through the engine (src/engine.ts)",
  },
  {
    name: "abi-outside-worker",
    files: (root) => source(root).filter((p) => p !== "web/src/bus.worker.ts" && notTypes(p)),
    pattern: /WebAssembly\.|transit_invoke/,
    fix: "loads or calls a module outside the bus worker; ask through the engine (src/engine.ts)",
  },
  {
    // docs/architecture/hosts.md: the ops answer which plan is published and
    // which stop is retired.
    name: "status-test-in-host",
    files: hosts,
    pattern: /status\)? *[!=]==? *"(retired|published)"|"(retired|published)" *[!=]==? *[A-Za-z_.]*status/,
    fix: "tests a lifecycle status; ask the op that owns the rule",
  },
  {
    // web/src/uis/inputs.ts reads each declared input with one poll and a
    // time limit.
    name: "reads-outside-inputs",
    files: hosts,
    pattern: /fetch\(|whileVisible/,
    fix: "fetches or polls by hand; declare the read as an input in the UI's ui.json",
  },
];

export function parity(root) {
  return RULES.flatMap((rule) => matching(root, rule.files(root), rule.pattern).map((h) => `${rule.name}: ${h.path}:${h.line} ${rule.fix}`));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
  process.exitCode = report("WEB GATE FAIL (web/gates/parity.mjs)", parity(root));
}
