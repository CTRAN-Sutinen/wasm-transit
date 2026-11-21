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
  for (const doc of registry("documents.json").documents.filter((d) => d.written)) {
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

// The architecture page draws from cargo's view of the workspace and
// from the registry. These tests read the same sources, so a crate, a
// dependency, or a read that the page leaves out fails here.
const metadata = JSON.parse(
  execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps", "--offline"], { cwd: ROOT, encoding: "utf8", maxBuffer: 64 << 20 }),
);
const drawn = (name) => /^(transit-|cap-|module-ui-)/.test(name);
const crates = metadata.packages.filter((p) => drawn(p.name));
const names = new Set(crates.map((p) => p.name));
const svgOf = (html, id) => html.slice(html.indexOf(`<svg class="g ${id}"`), html.indexOf("</svg>", html.indexOf(`<svg class="g ${id}"`)));

test("the crate map draws every crate, and a line for each normal dependency between them", () => {
  const html = page("architecture.html");
  const map = svgOf(html, "map");
  for (const p of crates) {
    assert.ok(map.includes(`class="node`) && map.includes(`data-id="${p.name}"`), `the map draws ${p.name}`);
    assert.ok(html.includes(`data-for="${p.name}"`), `${p.name} has a panel`);
    for (const d of p.dependencies.filter((d) => d.kind === null && names.has(d.name))) {
      assert.ok(map.includes(`data-from="${p.name}" data-to="${d.name}"`), `the map draws ${p.name} → ${d.name}`);
    }
  }
  const lines = [...map.matchAll(/class="edge/g)].length;
  const deps = crates.reduce((n, p) => n + p.dependencies.filter((d) => d.kind === null && names.has(d.name)).length, 0);
  assert.equal(lines, deps, "one line for each dependency, and no other");
});

test("on the crate map, every line points to a column on its left", () => {
  const map = svgOf(page("architecture.html"), "map");
  const x = new Map([...map.matchAll(/data-id="([^"]+)"[^>]*><rect class="pill" x="([\d.]+)"/g)].map((m) => [m[1], Number(m[2])]));
  for (const [, from, to] of map.matchAll(/class="edge[^"]*" data-from="([^"]+)" data-to="([^"]+)"/g)) {
    assert.ok(x.get(to) < x.get(from), `${from} → ${to} points left`);
  }
});

test("the bus draws a line for each kind of document each capability reads", () => {
  const html = page("architecture.html");
  const bus = svgOf(html, "bus");
  let reads = 0;
  for (const file of readdirSync(join(ROOT, "registry", "capabilities"))) {
    const record = registry(`capabilities/${file}`);
    assert.ok(bus.includes(`data-id="cap:${record.id}"`), `the bus draws ${record.id}`);
    for (const kind of record.documents ?? []) {
      reads++;
      assert.match(bus, new RegExp(`data-from="cap:${record.id}" data-to="kind:[a-z_]+"`), `${record.id} reads ${kind}`);
    }
  }
  assert.equal([...bus.matchAll(/class="edge/g)].length, reads, "one line for each read");
});

test("the UI table marks each capability that each UI links", () => {
  const html = page("architecture.html");
  for (const p of crates.filter((p) => p.name.startsWith("module-ui-"))) {
    for (const d of p.dependencies.filter((d) => d.kind === null && d.name.startsWith("cap-") && d.name !== "cap-abi")) {
      assert.ok(html.includes(`data-ui="${p.name}" data-cap="${d.name.slice(4)}"`), `${p.name} links ${d.name}`);
    }
  }
});

test("each trace step names a file and a text in it that exist, so a rename breaks the trace", () => {
  const html = page("architecture.html");
  const unescape = (t) => t.replace(/&quot;/g, '"').replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");
  for (const id of ["request", "write"]) {
    const section = html.slice(html.indexOf(`id="trace-${id}"`), html.indexOf("</section>", html.indexOf(`id="trace-${id}"`)));
    const steps = Number(/data-steps="(\d+)"/.exec(section)[1]);
    assert.equal([...section.matchAll(/<g class="step" data-step=/g)].length, steps, `${id}: a shape for each step`);
    assert.equal([...section.matchAll(/<li data-step=/g)].length, steps, `${id}: a list item for each step`);
    const refs = [...section.matchAll(/<code data-file="([^"]+)">[^<]*<\/code> <code class="sym">([^<]*)<\/code>/g)];
    assert.ok(refs.length >= steps, `${id}: every step names code`);
    for (const [, file, text] of refs) {
      const path = join(ROOT, unescape(file));
      assert.ok(existsSync(path), `${id}: ${file} exists`);
      assert.ok(readFileSync(path, "utf8").includes(unescape(text)), `${id}: ${file} still says ${unescape(text)}`);
    }
  }
});

test("the bucket map places every kind in the kind table", () => {
  const html = page("architecture.html");
  const map = html.slice(html.indexOf('class="bucketmap"'), html.indexOf('class="trace bucket"'));
  const table = JSON.parse(readFileSync(join(ROOT, "crates/cap-abi/kinds.json"), "utf8")).kinds;
  for (const k of table) assert.ok(map.includes(`data-kind="${k.store_kind}"`), `the bucket map places ${k.store_kind}`);
});

for (const id of ["story-note", "story-feed", "story-plan"]) test(`the bucket story ${id} names code that exists, and every row appears at a step it has`, () => {
  const html = page("architecture.html");
  assert.ok(html.includes(`id="${id}"`), `${id} is on the page`);
  const section = html.slice(html.indexOf(`id="${id}"`), html.indexOf("</section>", html.indexOf(`id="${id}"`)));
  const steps = Number(/data-steps="(\d+)"/.exec(section)[1]);
  assert.equal([...section.matchAll(/<li data-step=/g)].length, steps, "a list item for each step");
  for (const [, n] of section.matchAll(/data-(?:at|only)="(\d+)"/g)) assert.ok(Number(n) >= 1 && Number(n) <= steps, `row step ${n} exists`);
  const rows = new Set([...section.matchAll(/data-row="([^"]+)"/g)].map((m) => m[1]));
  for (const [, hot] of section.matchAll(/data-hot="([^"]+)"/g)) for (const id of hot.split(" ")) assert.ok(rows.has(id), `hot row ${id} exists`);
  const unescape = (t) => t.replace(/&quot;/g, '"').replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");
  const refs = [...section.matchAll(/<code data-file="([^"]+)">[^<]*<\/code> <code class="sym">([^<]*)<\/code>/g)];
  assert.ok(refs.length >= steps, "every step names code");
  for (const [, file, text] of refs) {
    const path = join(ROOT, unescape(file));
    assert.ok(existsSync(path), `${file} exists`);
    assert.ok(readFileSync(path, "utf8").includes(unescape(text)), `${file} still says ${unescape(text)}`);
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
  for (const doc of registry("documents.json").documents.filter((d) => d.written)) {
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

test("a list in an op description renders as a nested list, and every edit of author apply is listed", () => {
  const html = page("cap-author.html");
  const apply = html.slice(html.indexOf('id="op-apply"'));
  assert.match(apply, /<ul>\s*<li><code>set_plan/, "a bullet list of edits");
  assert.match(apply, /<li>[^]*<ul>[^]*<\/ul>[^]*<\/li>/, "a nested list");
  assert.ok(!/<p>- /.test(apply), "no bullet is left as plain text");
  // Each edit the Rust enum defines (docs/units: the edit vocabulary
  // lives in transit-network) is named in the description. The
  // upsert_ and remove_ edits of the six entity kinds are named by
  // their pattern, so they are not checked one by one.
  const source = readFileSync(join(ROOT, "crates/transit-network/src/edit.rs"), "utf8");
  const enumBody = source.slice(source.indexOf("pub enum NetworkEdit"), source.indexOf("\n}\n", source.indexOf("pub enum NetworkEdit")));
  const edits = [...enumBody.matchAll(/^    ([A-Z][A-Za-z]+)\b/gm)].map((m) => m[1].replace(/(?<!^)([A-Z])/g, "_$1").toLowerCase());
  assert.ok(edits.length > 50, "found the edits");
  const missing = edits.filter((e) => !/^(upsert|remove)_(stop|route|pattern|service_type|pattern_trips|block)$/.test(e) && !apply.includes(e));
  assert.deepEqual(missing, [], "edits the description never names");
});
