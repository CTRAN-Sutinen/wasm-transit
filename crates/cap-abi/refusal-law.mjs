// Pins the unknown_document refusal (docs/architecture/bus.md, "Answers").
// Every worked example with a `*_hash` argument runs on a fresh module that
// holds nothing. An op that reads the document refuses with that code, one
// of those hashes, and a load op the kernel has. An op that only records the
// hash, such as a pick's network, answers as usual.
// Run as a script, this checks the built modules in web/public/caps.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { shell } from "./independence.mjs";

const HASH = /^sha256:[0-9a-f]{64}$/;

export function evictedExamples({ manifest, instantiate }) {
  const loads = new Set((manifest.kernel_ops ?? []).map((op) => op.name).filter((n) => n.startsWith("load_")));
  const failures = [];
  let checked = 0;
  let refused = 0;
  for (const cap of manifest.capabilities) {
    for (const [op, doc] of Object.entries(cap.op_docs ?? {})) {
      const request = doc.example?.request;
      if (!request) continue;
      // A document crosses by hash as a top-level `*_hash` argument. A hash
      // inside an inline document is a record's identity, not a document.
      const named = new Set(
        Object.entries(request)
          .filter(([key, value]) => key.endsWith("_hash") && typeof value === "string" && HASH.test(value))
          .map(([, value]) => value),
      );
      if (named.size === 0) continue;
      checked += 1;
      const outcome = JSON.parse(instantiate(cap).invoke(request));
      const error = outcome.error;
      const where = `${cap.id}.${op}`;
      if (!error) continue;
      refused += 1;
      if (error.code !== "unknown_document") failures.push(`${where} answered ${JSON.stringify(error)}, not unknown_document`);
      else if (!named.has(error.hash)) failures.push(`${where} names ${error.hash}, which the request does not`);
      else if (!loads.has(error.load)) failures.push(`${where} names load ${error.load}, which the kernel lacks`);
    }
  }
  if (refused === 0) failures.push("no worked example refused: the law checked nothing");
  return { checked, refused, failures };
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
  const { checked, refused, failures } = evictedExamples({ manifest, instantiate });
  if (failures.length) {
    for (const f of failures) console.error("REFUSAL FAIL: " + f);
    process.exit(1);
  }
  console.log(`unknown_document: ${refused} of ${checked} worked examples with a hash refused by code with nothing registered`);
}
