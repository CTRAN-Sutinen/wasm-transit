// The generated UIs (docs/architecture/generated-uis.md): each
// web/src/uis/<id>/ carries its descriptor, its page, and its e2e spec,
// and owns its host layer. Nothing reaches into another UI's directory,
// and no component imports a UI's host. uis.test.mjs is the broken
// fixture.

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { files, matching, report } from "../../tools/gates/files.mjs";

const FIELDS = ["id", "title", "summary", "questions", "components"];

export function uis(root) {
  const problems = [];
  const dir = join(root, "web/src/uis");
  const ids = readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isDirectory() && !e.name.startsWith("."))
    .map((e) => e.name)
    .sort();
  for (const id of ids) {
    for (const need of [
      `web/src/uis/${id}/ui.json`,
      `web/${id}.html`,
      `web/e2e/ui-${id}.spec.mjs`,
    ]) {
      if (!existsSync(join(root, need)))
        problems.push(`generated UI ${id} is missing ${need}`);
    }
    const descriptor = join(dir, id, "ui.json");
    if (!existsSync(descriptor)) continue;
    let j;
    try {
      j = JSON.parse(readFileSync(descriptor, "utf8"));
    } catch (e) {
      problems.push(`web/src/uis/${id}/ui.json: ${e.message}`);
      continue;
    }
    for (const k of FIELDS)
      if (j[k] === undefined)
        problems.push(`web/src/uis/${id}/ui.json: missing ${k}`);
    if (j.id !== id)
      problems.push(
        `web/src/uis/${id}/ui.json: id ${j.id} is not its directory's`,
      );
  }
  const own = files(root, "web/src/uis").filter(
    (p) =>
      p.split("/").length === 5 && !p.split("/").some((s) => s.startsWith(".")),
  );
  for (const h of matching(root, own, /from "\.\.\/[a-z-]+\//)) {
    problems.push(
      `${h.path}:${h.line} imports another generated UI's files; each UI owns its host layer (src/uis/<id>/host.svelte.ts)`,
    );
  }
  const components = files(root, "web/src/components").filter((p) =>
    /^web\/src\/components\/[^/]+\.(svelte|ts)$/.test(p),
  );
  for (const h of matching(root, components, /from "\.\.\/uis\//)) {
    problems.push(
      `${h.path}:${h.line} imports a generated UI's host layer; take node values as props`,
    );
  }
  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
  process.exitCode = report("UI GATE FAIL (web/gates/uis.mjs)", uis(root));
}
