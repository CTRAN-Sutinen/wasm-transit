#!/usr/bin/env node
// The failure tables and the inputs (docs/architecture/generated-uis.md,
// the procedure's step 5; each UI's file in docs/architecture/generated-uis/;
// web/src/uis/inputs.ts).
//
// Each generated UI declares its inputs in its ui.json (`inputs`). Each
// input names the rows of the UI's failure table that say what the page
// does while its reads fail (`fails_as`). This file reads both as data.
//
// - `uiDocs(root)`: generated-uis.md, then each UI's file under
//   generated-uis/, as one text.
// - `failureTables(markdown)`: each UI's failure table in that text, by
//   its id. A row's name is its first cell, to the colon. A UI whose
//   section says "Its failure table is <other>'s" has the other UI's
//   rows.
// - `declaredInputs(root)`: each UI's declared inputs, by its id.
// - `check(root)`: the gate. It lists each declared input whose
//   `fails_as` names no row of its UI's table, and each UI that
//   declares inputs and has no table.
//
// Run as a script, it runs the gate and exits 1 on a finding
// (`just test`, the web gates). The docs site (tools/docs-site.mjs)
// draws the same data.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SECTION = /^`([a-z][a-z0-9-]*)` \(`src\/uis\/\1\/`\)/;
const HEADER = /^\| Input, and how it fails \|/;
const SAME = /Its failure table is ([a-z][a-z -]*)'s\b/;

export function rowName(cell) {
  const i = cell.indexOf(":");
  return (i < 0 ? cell : cell.slice(0, i)).trim();
}

export function failureTables(markdown) {
  const lines = markdown.split("\n");
  const tables = {};
  let ui = null;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const section = SECTION.exec(line);
    if (section) {
      ui = section[1];
      tables[ui] ??= { rows: [], same: null };
      continue;
    }
    if (!ui) continue;
    const same = SAME.exec(line);
    if (same) tables[ui].same = same[1].trim().replace(/ /g, "-");
    if (!HEADER.test(line)) continue;
    for (i += 2; i < lines.length && lines[i].startsWith("|"); i++) {
      const cells = lines[i].split("|").slice(1, -1).map((c) => c.trim());
      tables[ui].rows.push({ name: rowName(cells[0]), cells });
    }
  }
  for (const id of Object.keys(tables)) {
    const same = tables[id].same;
    if (same && tables[same] && tables[id].rows.length === 0) tables[id].rows = tables[same].rows;
  }
  return tables;
}

export function uiDocs(root) {
  const main = path.join(root, "docs/architecture/generated-uis.md");
  const dir = path.join(root, "docs/architecture/generated-uis");
  const files = fs.existsSync(dir)
    ? fs.readdirSync(dir).filter((f) => f.endsWith(".md")).sort().map((f) => path.join(dir, f))
    : [];
  return [main, ...files].map((f) => fs.readFileSync(f, "utf8")).join("\n");
}

export function declaredInputs(root) {
  const uis = path.join(root, "web/src/uis");
  const out = {};
  for (const id of fs.readdirSync(uis).sort()) {
    const file = path.join(uis, id, "ui.json");
    if (!fs.existsSync(file)) continue;
    const json = JSON.parse(fs.readFileSync(file, "utf8"));
    if (Array.isArray(json.inputs)) out[id] = json.inputs;
  }
  return out;
}

export function check(root) {
  const tables = failureTables(uiDocs(root));
  const findings = [];
  for (const [ui, inputs] of Object.entries(declaredInputs(root))) {
    const table = tables[ui];
    if (!table || table.rows.length === 0) {
      findings.push(`${ui} declares inputs and has no failure table`);
      continue;
    }
    const names = new Set(table.rows.map((r) => r.name));
    for (const input of inputs) {
      if (!Array.isArray(input.fails_as) || input.fails_as.length === 0) {
        findings.push(`${ui}: input ${input.id} names no failure-table row (fails_as)`);
        continue;
      }
      for (const name of input.fails_as) {
        if (!names.has(name)) findings.push(`${ui}: input ${input.id} names the row "${name}", which its failure table does not have`);
      }
    }
  }
  return findings;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const findings = check(root);
  for (const f of findings) console.error(`FAILURE-TABLE GATE FAIL: ${f} (docs/architecture/generated-uis/)`);
  process.exit(findings.length ? 1 : 0);
}
