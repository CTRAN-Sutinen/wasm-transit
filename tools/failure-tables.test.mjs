// Pins tools/failure-tables.mjs: a row's name, a UI that shares another's
// table, and a declared input whose row is missing.

import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { check, failureTables, rowName, uiDocs } from "./failure-tables.mjs";

const doc = `
\`alpha\` (\`src/uis/alpha/\`) is a page.

Its failure table.

| Input, and how it fails | The page knows by | Still shows |
|---|---|---|
| The server: no answer | a fetch error | the last copy |
| The bucket: no answer (the server answers) | \`X-Store-Stale\` | detours |

\`beta\` (\`src/uis/beta/\`) is another page.

Its failure table is alpha's.
`;

test("a row's name is its first cell, to the colon", () => {
  assert.equal(rowName("The bucket: no answer (the server answers)"), "The bucket");
  assert.equal(rowName("Sign-in"), "Sign-in");
});

test("each UI's table, and a UI that has another's", () => {
  const t = failureTables(doc);
  assert.deepEqual(t.alpha.rows.map((r) => r.name), ["The server", "The bucket"]);
  assert.deepEqual(t.beta.rows.map((r) => r.name), ["The server", "The bucket"]);
});

function tree(inputs, ui = "alpha") {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "failure-tables-"));
  fs.mkdirSync(path.join(root, "docs/architecture"), { recursive: true });
  fs.writeFileSync(path.join(root, "docs/architecture/generated-uis.md"), doc);
  fs.mkdirSync(path.join(root, "web/src/uis", ui), { recursive: true });
  fs.writeFileSync(path.join(root, "web/src/uis", ui, "ui.json"), JSON.stringify({ id: ui, inputs }));
  return root;
}

test("the gate passes when every input names a row its table has", () => {
  assert.deepEqual(check(tree([{ id: "edits", fails_as: ["The server", "The bucket"] }])), []);
});

test("the gate finds an input whose row is missing, and one that names none", () => {
  const findings = check(tree([{ id: "edits", fails_as: ["The cache"] }, { id: "live" }]));
  assert.equal(findings.length, 2);
  assert.match(findings[0], /edits.*The cache/);
  assert.match(findings[1], /live names no failure-table row/);
});

test("the gate finds a UI that declares inputs and has no table", () => {
  assert.match(check(tree([{ id: "edits", fails_as: ["The server"] }], "gamma"))[0], /gamma declares inputs and has no failure table/);
});

test("a UI's table in its own file under generated-uis/ is read, and may share the main file's", () => {
  const root = tree([{ id: "edits", fails_as: ["The cache"] }], "gamma");
  fs.mkdirSync(path.join(root, "docs/architecture/generated-uis"));
  fs.writeFileSync(path.join(root, "docs/architecture/generated-uis/gamma.md"), `# gamma

\`gamma\` (\`src/uis/gamma/\`) is a page in its own file.

| Input, and how it fails | The page knows by | Still shows |
|---|---|---|
| The cache: gone | a miss | nothing |
`);
  fs.writeFileSync(path.join(root, "docs/architecture/generated-uis/delta.md"), `# delta

\`delta\` (\`src/uis/delta/\`) is another page.

Its failure table is alpha's.
`);
  assert.deepEqual(check(root), []);
  const t = failureTables(uiDocs(root));
  assert.deepEqual(t.delta.rows.map((r) => r.name), ["The server", "The bucket"]);
});
