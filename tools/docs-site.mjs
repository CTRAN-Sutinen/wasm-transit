// The docs site: static HTML generated from the registry.
//
//   node tools/docs-site.mjs [OUT] [--serve [PORT]]
//
// OUT defaults to docs-site/. With --serve, the script then serves OUT
// on 127.0.0.1 (PORT defaults to 8085) until you stop it.
// `just docs` and `just docs serve` run it. The pages read registry/ and nothing else, so
// they match the modules that `just caps` built. The registry holds
// each capability's purpose, its op descriptions, the computed worked
// examples, and the schema and example of each document kind. There is
// no hand-written content here. To change a description, edit the
// capability's OPS.md and run `just caps`.
//
// Pages: index.html, kernel.html, cap-<id>.html, doc-<kind>.html.
import { createServer } from "node:http";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const REGISTRY = join(ROOT, "registry");
const args = process.argv.slice(2);
const serveAt = args.indexOf("--serve");
const port = serveAt < 0 ? null : Number(args[serveAt + 1] ?? 8085);
if (port !== null && !(port >= 0 && port < 65536)) throw new Error("--serve takes a port number");
const positional = args.filter((a, i) => !a.startsWith("--") && !(serveAt >= 0 && i === serveAt + 1));
const out = positional[0] ?? join(ROOT, "docs-site");

const read = (path) => JSON.parse(readFileSync(join(REGISTRY, path), "utf8"));
const pointer = read("registry.json");
const documents = read("documents.json").documents;
const capabilities = readdirSync(join(REGISTRY, "capabilities"))
  .filter((f) => f.endsWith(".json"))
  .map((f) => read(`capabilities/${f}`))
  .sort((a, b) => a.id.localeCompare(b.id));
const docKinds = new Set(documents.map((d) => d.kind));

const esc = (text) =>
  String(text ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

// Text from the registry: paragraphs split at blank lines, `code`
// spans, and lists. A block whose lines start with "- " is a list. A
// line indented by two spaces and starting with "- " is an item of the
// list under the item above it. Another indented line continues the
// item above it.
const inline = (text) => esc(text).replace(/`([^`]+)`/g, "<code>$1</code>");

function list(lines) {
  const items = [];
  for (const line of lines) {
    const nested = /^ {2}- (.*)$/.exec(line);
    const top = /^- (.*)$/.exec(line);
    if (top) items.push({ text: top[1], kids: [] });
    else if (nested && items.length) items.at(-1).kids.push(nested[1]);
    else if (items.length) {
      const item = items.at(-1);
      if (item.kids.length) item.kids[item.kids.length - 1] += ` ${line.trim()}`;
      else item.text += ` ${line.trim()}`;
    }
  }
  const html = (item) =>
    `<li>${inline(item.text)}${item.kids.length ? `<ul>${item.kids.map((k) => `<li>${inline(k)}</li>`).join("")}</ul>` : ""}</li>`;
  return `<ul>${items.map(html).join("")}</ul>`;
}

const prose = (text) =>
  String(text ?? "")
    .split(/\n\s*\n/)
    .map((block) => {
      const lines = block.replace(/\s+$/, "").split("\n");
      return lines[0].startsWith("- ") ? list(lines) : `<p>${inline(block.trim())}</p>`;
    })
    .join("\n");

const raw = (value) => `<pre>${esc(JSON.stringify(value, null, 2))}</pre>`;

// A JSON value as a collapsible tree, like a browser's JSON viewer.
// The root and its children start open. A closed object or array shows
// a preview of its first entries.
function tree(value, key, depth = 0) {
  const label = key === undefined ? "" : `<span class="k">${esc(JSON.stringify(key))}</span>: `;
  if (value === null || typeof value !== "object") {
    const kind = value === null ? "null" : typeof value;
    return `<div class="row">${label}<span class="v-${kind}">${esc(JSON.stringify(value))}</span></div>`;
  }
  const isArray = Array.isArray(value);
  const entries = isArray ? value.map((v, i) => [i, v]) : Object.entries(value);
  const [open, close] = isArray ? ["[", "]"] : ["{", "}"];
  if (entries.length === 0) return `<div class="row">${label}${open}${close}</div>`;
  const kids = entries.map(([k, v]) => tree(v, isArray ? undefined : k, depth + 1)).join("");
  const preview = entries
    .filter(([, v]) => v === null || typeof v !== "object")
    .slice(0, 3)
    .map(([k, v]) => (isArray ? "" : `${k}: `) + JSON.stringify(v))
    .join(", ");
  const count = `${entries.length} ${isArray ? "items" : "keys"}`;
  return `<details${depth < 2 ? " open" : ""}><summary>${label}${open} <span class="muted">${esc(count)}${preview ? ` · ${esc(preview.slice(0, 80))}` : ""}</span></summary>` +
    `<div class="kids">${kids}</div><div class="row">${close}</div></details>`;
}

// A JSON value as a tree, with the raw text in a closed block.
const json = (value) =>
  `<div class="jt">${tree(value)}</div><details class="rawblock" id="raw"><summary>Raw JSON</summary>${raw(value)}</details>`;

// A JSON Schema as a property tree, like json-schema-for-humans and
// Stoplight Elements. A `$ref` links to its definition below. The
// schemas that schemars writes use objects, arrays, maps, enums,
// constants, and `oneOf` of constants or objects, and nothing else.
function typeLabel(s) {
  if (s === true || s === undefined) return "any";
  if (s.$ref) {
    const name = s.$ref.split("/").pop();
    return `<a href="#def-${esc(name)}">${esc(name)}</a>`;
  }
  if ("const" in s) return `<code>${esc(JSON.stringify(s.const))}</code>`;
  if (s.enum) return s.enum.map((v) => `<code>${esc(JSON.stringify(v))}</code>`).join(" | ");
  const variants = s.oneOf ?? s.anyOf;
  if (variants) return variants.map(typeLabel).join(" | ");
  const types = [].concat(s.type ?? []);
  const parts = types.map((t) => {
    if (t === "array") return `array of ${typeLabel(s.items)}`;
    if (t === "object" && s.additionalProperties && !s.properties) return `map of ${typeLabel(s.additionalProperties)}`;
    return esc(t);
  });
  const extra = [s.format, s.minimum !== undefined ? `min ${s.minimum}` : null].filter(Boolean).join(", ");
  return (parts.join(" | ") || "object") + (extra ? ` <span class="muted">(${esc(extra)})</span>` : "");
}

// The inline parts of a schema that need their own rows: its
// properties, the properties of its array items or map values, and the
// variants of a `oneOf`.
function schemaChildren(s, depth) {
  if (!s || s === true || s.$ref) return "";
  let html = "";
  if (s.properties) {
    const required = new Set(s.required ?? []);
    html += Object.entries(s.properties).map(([name, sub]) => schemaRow(name, sub, required.has(name), depth)).join("");
  }
  const inner = s.items ?? (typeof s.additionalProperties === "object" ? s.additionalProperties : null);
  if (inner) html += schemaChildren(inner, depth);
  for (const variant of s.oneOf ?? s.anyOf ?? []) {
    if ("const" in variant) {
      html += `<div class="row"><code>${esc(JSON.stringify(variant.const))}</code> ${variant.description ? `<span class="muted">${esc(variant.description)}</span>` : ""}</div>`;
    } else if (variant.properties) {
      html += `<details><summary><span class="muted">variant</span> <span class="type">${typeLabel(variant)}</span></summary><div class="kids">${schemaChildren(variant, depth + 1)}</div></details>`;
    }
  }
  return html;
}

function schemaRow(name, s, required, depth) {
  const head = `<span class="name" data-prop="${esc(name)}">${esc(name)}</span>` +
    `${required ? ' <span class="req">required</span>' : ""} <span class="type">${typeLabel(s)}</span>`;
  const description = s.description ? `<div class="desc">${prose(s.description)}</div>` : "";
  const kids = schemaChildren(s, depth + 1);
  if (!kids) return `<div class="row">${head}${description}</div>`;
  return `<details${depth === 0 ? " open" : ""}><summary>${head}</summary>${description}<div class="kids">${kids}</div></details>`;
}

function schemaBody(s) {
  const kids = schemaChildren(s, 0);
  const head = `<p class="type">${typeLabel(s)}</p>${s.description ? `<div class="desc">${prose(s.description)}</div>` : ""}`;
  return head + (kids ? `<div class="kids">${kids}</div>` : "");
}

function schemaView(schema) {
  const defs = Object.entries(schema.$defs ?? {})
    .map(([name, def]) => `<section id="def-${esc(name)}"><h3>${esc(name)}</h3>${schemaBody(def)}</section>`)
    .join("\n");
  return `${schemaBody(schema)}${defs ? `<h2>Definitions</h2>\n${defs}` : ""}`;
}

const kindLink = (kind) => (docKinds.has(kind) ? `<a href="doc-${esc(kind)}.html">${esc(kind)}</a>` : esc(kind));

const STYLE = `
:root { --key: #0b5cad; --str: #1a7f37; --num: #953800; --lit: #8250df; --warn: #b3261e; --bg: #fff; --fg: #1d1d1f; --muted: #5f6368; --line: #d8dadf; --code: #f3f4f6; --link: #0b5cad; }
@media (prefers-color-scheme: dark) {
  :root { --key: #7ab7ff; --str: #7ee787; --num: #ffa657; --lit: #d2a8ff; --warn: #ff8a80; --bg: #16181d; --fg: #e8e8ea; --muted: #a0a4ab; --line: #343841; --code: #20232a; --link: #7ab7ff; }
}
body { background: var(--bg); color: var(--fg); font: 16px/1.55 system-ui, sans-serif; margin: 0; }
header, main { max-width: 56rem; margin: 0 auto; padding: 0 16px; }
header { border-bottom: 1px solid var(--line); padding-top: 12px; padding-bottom: 12px; }
header a { margin-right: 16px; font-weight: 600; }
a { color: var(--link); }
h1 { margin-top: 24px; }
h2 { margin-top: 32px; border-bottom: 1px solid var(--line); padding-bottom: 4px; }
code, pre { background: var(--code); border-radius: 4px; font-size: 0.9em; }
code { padding: 1px 4px; }
pre { padding: 12px; overflow-x: auto; max-height: 32rem; }
details { margin: 8px 0; }
summary { cursor: pointer; }
.rawblock > summary, details[id^="example-"] > summary { color: var(--muted); }
.muted { color: var(--muted); }
.jt, .kids { font: 0.85em/1.5 ui-monospace, monospace; }
.kids { margin-left: 1.2em; border-left: 1px solid var(--line); padding-left: 10px; }
.jt .row, .jt summary { white-space: pre-wrap; word-break: break-word; }
.k { color: var(--key); }
.v-string { color: var(--str); }
.v-number { color: var(--num); }
.v-boolean, .v-null { color: var(--lit); }
.name { font-weight: 600; font-family: ui-monospace, monospace; }
.type { color: var(--muted); }
.req { color: var(--warn); font-size: 0.75em; text-transform: uppercase; }
.desc { color: var(--muted); font-size: 0.9em; margin: 2px 0 6px; }
.desc p { margin: 4px 0; }
section[id^="def-"] { border-top: 1px solid var(--line); margin-top: 16px; padding-top: 4px; }
section[id^="def-"]:target { background: var(--code); }
.rawblock { margin-top: 8px; }
table { border-collapse: collapse; width: 100%; }
td, th { border-bottom: 1px solid var(--line); padding: 6px 8px; text-align: left; vertical-align: top; }
footer { color: var(--muted); font-size: 0.85em; margin-top: 48px; padding-bottom: 32px; }
`;

function layout(title, body) {
  const e = pointer.evidence ?? {};
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(title)} · wasm-transit</title>
<style>${STYLE}</style>
</head>
<body>
<header><a href="index.html">wasm-transit</a><a href="kernel.html">Kernel ops</a></header>
<main>
${body}
<footer>Generated from the registry. Commit ${esc((e.source_commit ?? "unknown").slice(0, 10))}, profile ${esc(e.profile ?? "unknown")}.</footer>
</main>
</body>
</html>
`;
}

function indexPage() {
  const caps = capabilities
    .map(
      (c) => `<tr><td><a href="cap-${esc(c.id)}.html">${esc(c.id)}</a></td>` +
        `<td>${esc(firstSentence(c.purpose))}</td><td>${c.ops.length}</td></tr>`,
    )
    .join("\n");
  const docs = documents
    .map((d) => `<tr><td><a href="doc-${esc(d.kind)}.html">${esc(d.kind)}</a></td><td>v${d.schema_version}</td></tr>`)
    .join("\n");
  return layout(
    "Capabilities",
    `<h1>Capabilities and documents</h1>
<p class="muted">Each capability is a WebAssembly module that answers ops over documents. Every host runs the same modules. Run an op with <code>node tools/caps-run.mjs</code>.</p>
<h2>Capabilities</h2>
<table><tr><th>Capability</th><th>Purpose</th><th>Ops</th></tr>
${caps}
</table>
<h2>Document kinds</h2>
<table><tr><th>Kind</th><th>Version</th></tr>
${docs}
</table>
<p>The <a href="kernel.html">kernel ops</a> load documents. Every module answers them.</p>`,
  );
}

function firstSentence(text) {
  const match = /^(.+?[.!?])(?=\s+[A-Z(`"]|$)/s.exec(text ?? "");
  const sentence = (match ? match[1] : text ?? "").replace(/\s+/g, " ").trim();
  return sentence.length > 240 ? `${sentence.slice(0, 237)}…` : sentence;
}

function capabilityPage(c) {
  const ops = c.ops
    .map((op) => {
      const doc = c.op_docs?.[op] ?? {};
      const example = doc.example
        ? `<details id="example-${esc(op)}"><summary>Example</summary>
<p><strong>Request</strong></p>${json(doc.example.request)}
<p><strong>Response</strong></p>${json(doc.example.response)}</details>`
        : "";
      return `<h3 id="op-${esc(op)}"><code>${esc(op)}</code></h3>
${prose(doc.description ?? "No description.")}
${example}`;
    })
    .join("\n");
  const kinds = (c.documents ?? []).map(kindLink).join(", ") || "none";
  return layout(
    c.id,
    `<h1>${esc(c.id)}</h1>
${prose(c.purpose)}
<p><strong>Documents it reads:</strong> ${kinds}</p>
<h2>Ops</h2>
<p class="muted">${c.ops.map((op) => `<a href="#op-${esc(op)}">${esc(op)}</a>`).join(" · ")}</p>
${ops}`,
  );
}

function kernelPage() {
  const ops = (pointer.kernel_ops ?? [])
    .map(
      (k) => `<h3 id="op-${esc(k.name)}"><code>${esc(k.name)}</code> <span class="muted">code <span>${k.code}</span></span></h3>
${prose(k.description)}`,
    )
    .join("\n");
  return layout(
    "Kernel ops",
    `<h1>Kernel ops</h1>
<p class="muted">Every module answers these. Each loads one kind of document under its content hash.</p>
${ops}`,
  );
}

function documentPage(d) {
  const schema = read(d.schema);
  const example = read(d.example);
  return layout(
    d.kind,
    `<h1>${esc(d.kind)} <span class="muted">v${d.schema_version}</span></h1>
${prose(schema.description ?? "")}
<p class="muted">Schema ${esc(d.schema)}. The store envelope kind is ${esc(d.store_kind ?? "none (not stored)")}.</p>
<h2>Schema</h2>
<div id="schema">${schemaView(schema)}</div>
<details class="rawblock"><summary>Raw schema</summary>${raw(schema)}</details>
<h2>Example</h2>
<div id="example">${json(example)}</div>`,
  );
}

mkdirSync(out, { recursive: true });
const write = (name, html) => writeFileSync(join(out, name), html);
write("index.html", indexPage());
write("kernel.html", kernelPage());
for (const c of capabilities) write(`cap-${c.id}.html`, capabilityPage(c));
for (const d of documents) write(`doc-${d.kind}.html`, documentPage(d));
console.log(`docs site: ${capabilities.length} capabilities, ${documents.length} document kinds → ${out}`);

// The pages are static files: the server reads one file for each
// request, and only inside OUT.
if (port !== null) {
  const server = createServer((request, response) => {
    const path = decodeURIComponent(new URL(request.url, "http://x").pathname);
    const name = path === "/" ? "index.html" : path.slice(1);
    if (!/^[A-Za-z0-9._-]+\.html$/.test(name)) {
      response.writeHead(404).end("not found");
      return;
    }
    let body;
    try {
      body = readFileSync(join(out, name));
    } catch {
      response.writeHead(404).end("not found");
      return;
    }
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" }).end(body);
  });
  server.listen(port, "127.0.0.1", () => console.log(`docs site: http://127.0.0.1:${server.address().port}/ (Ctrl+C stops it)`));
}
