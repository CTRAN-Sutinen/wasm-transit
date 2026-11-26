// The file walk that the source gates share.

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

// Every file under ROOT/dir, as a path relative to ROOT with "/".
export function files(root, dir) {
  const out = [];
  const walk = (d) => {
    if (!existsSync(d)) return;
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) walk(p);
      else out.push(relative(root, p).split("\\").join("/"));
    }
  };
  walk(join(root, dir));
  return out.sort();
}

// The files whose text matches, and the first matching line of each.
export function matching(root, paths, pattern) {
  const hits = [];
  for (const path of paths) {
    const lines = readFileSync(join(root, path), "utf8").split("\n");
    const at = lines.findIndex((l) => pattern.test(l));
    if (at >= 0) hits.push({ path, line: at + 1, text: lines[at].trim() });
  }
  return hits;
}

export function report(name, problems) {
  for (const p of problems) console.error(`${name}: ${p}`);
  return problems.length ? 1 : 0;
}
