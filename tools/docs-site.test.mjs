// The docs site: `node tools/docs-site.mjs OUT` writes static HTML from
// the registry. These tests check that the site lists every capability,
// every op, every kernel op, and every document kind, and that it links
// without a broken link.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const registry = (path) => JSON.parse(readFileSync(join(ROOT, "registry", path), "utf8"));
const out = mkdtempSync(join(tmpdir(), "docs-site-"));
execFileSync("node", [join(ROOT, "tools", "docs-site.mjs"), out], { stdio: "inherit" });
const page = (name) => readFileSync(join(out, name), "utf8");

test("the index lists each capability and links its page", () => {
  const index = page("index.html");
  for (const file of readdirSync(join(ROOT, "registry", "capabilities"))) {
    const id = file.replace(/\.json$/, "");
    assert.ok(index.includes(`href="cap-${id}.html"`), `index links ${id}`);
    assert.ok(existsSync(join(out, `cap-${id}.html`)), `page for ${id}`);
  }
});

test("a capability page holds its purpose and every op with its description and example", () => {
  for (const file of readdirSync(join(ROOT, "registry", "capabilities"))) {
    const record = registry(`capabilities/${file}`);
    const html = page(`cap-${record.id}.html`);
    assert.ok(html.includes(`<h1>${record.id}</h1>`), `${record.id} title`);
    for (const op of record.ops) {
      assert.ok(html.includes(`id="op-${op}"`), `${record.id} lists op ${op}`);
    }
    const withExample = record.ops.filter((op) => record.op_docs[op]?.example);
    for (const op of withExample) {
      assert.ok(html.includes(`id="example-${op}"`), `${record.id} shows the example of ${op}`);
    }
  }
});

test("the kernel page lists each kernel op by name and code", () => {
  const html = page("kernel.html");
  for (const op of registry("registry.json").kernel_ops) {
    assert.ok(html.includes(`id="op-${op.name}"`), `kernel op ${op.name}`);
    assert.ok(html.includes(`>${op.code}<`), `code of ${op.name}`);
  }
});

test("each document kind has a page with its schema and example", () => {
  for (const doc of registry("documents.json").documents) {
    const html = page(`doc-${doc.kind}.html`);
    assert.ok(html.includes(`v${doc.schema_version}`), `${doc.kind} version`);
    assert.ok(html.includes('id="schema"') && html.includes('id="example"'), `${doc.kind} schema and example`);
    assert.ok(page("index.html").includes(`href="doc-${doc.kind}.html"`), `index links ${doc.kind}`);
  }
});

test("every local link in every page points at a page that exists", () => {
  for (const name of readdirSync(out).filter((n) => n.endsWith(".html"))) {
    for (const [, target] of page(name).matchAll(/href="([^"#:]+\.html)(?:#[^"]*)?"/g)) {
      assert.ok(existsSync(join(out, target)), `${name} links ${target}`);
    }
  }
});

test("page text is escaped", () => {
  assert.ok(!/<script/i.test(page("index.html")));
});

test("--serve builds the site and serves it on the given port, on loopback only", async () => {
  const { createServer } = await import("node:net");
  const { spawn } = await import("node:child_process");
  const port = await new Promise((resolve) => {
    const probe = createServer().listen(0, "127.0.0.1", () => {
      const { port } = probe.address();
      probe.close(() => resolve(port));
    });
  });
  const served = mkdtempSync(join(tmpdir(), "docs-serve-"));
  const child = spawn("node", [join(ROOT, "tools", "docs-site.mjs"), served, "--serve", String(port)], { stdio: "ignore" });
  try {
    let response;
    for (let i = 0; i < 50 && !response; i++) {
      response = await fetch(`http://127.0.0.1:${port}/`).catch(() => null);
      if (!response) await new Promise((r) => setTimeout(r, 100));
    }
    assert.ok(response, "the server answers");
    assert.equal(response.status, 200);
    assert.match(await response.text(), /Capabilities and documents/);
    const missing = await fetch(`http://127.0.0.1:${port}/nope.html`);
    assert.equal(missing.status, 404);
    const escape = await fetch(`http://127.0.0.1:${port}/..%2fjustfile`);
    assert.equal(escape.status, 404, "a path outside the site is refused");
  } finally {
    child.kill();
  }
});

test("a schema shows as a property tree: each property, each definition, each reference resolved", () => {
  for (const doc of registry("documents.json").documents) {
    const schema = registry(doc.schema);
    const html = page(`doc-${doc.kind}.html`);
    for (const name of Object.keys(schema.properties ?? {})) {
      assert.ok(html.includes(`data-prop="${name}"`), `${doc.kind} shows property ${name}`);
    }
    for (const name of Object.keys(schema.$defs ?? {})) {
      assert.ok(html.includes(`id="def-${name}"`), `${doc.kind} shows definition ${name}`);
    }
    for (const [, target] of html.matchAll(/href="#(def-[^"]+)"/g)) {
      assert.ok(html.includes(`id="${target}"`), `${doc.kind} reference ${target} resolves`);
    }
  }
});

test("a required property is marked, and an enum shows its values", () => {
  const html = page("doc-operator_edits.html");
  assert.match(html, /class="req"/);
  assert.ok(html.includes("accepted") && html.includes("retired"), "EditStatus values");
});

test("an example shows as a collapsible tree, with the raw JSON beside it", () => {
  const html = page("doc-network.html");
  assert.match(html, /class="jt"/);
  assert.match(html, /<details/);
  assert.match(html, /id="raw"/);
});

test("example strings are escaped in the tree", () => {
  assert.ok(!/<img|<svg/i.test(page("doc-network.html")));
});
