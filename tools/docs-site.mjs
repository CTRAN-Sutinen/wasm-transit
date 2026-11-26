// The docs site is static HTML generated from the registry.
//
//   node tools/docs-site.mjs [OUT] [--serve [PORT]]
//
// OUT defaults to docs-site/. With --serve, the script then serves OUT
// on 127.0.0.1 (PORT defaults to 8085) until you stop it.
// `just docs` and `just docs serve` run it. The pages read registry/, so
// they match the modules that `just caps` built. The architecture page
// also reads the workspace's crates from `cargo metadata` (no build) and
// the kind table (crates/cap-abi/kinds.json).
// The inputs section draws each UI's declared inputs (its ui.json's
// `inputs`) beside the rows of its failure table that they fail as (each
// UI's file in docs/architecture/generated-uis/), read as data through
// tools/failure-tables.mjs. The only hand-written content is the two
// traces (`TRACES`) and the bucket stories (`BUCKET_STORIES`), and the
// test checks the code that each of their steps names. To change a
// description, edit the capability's OPS.md, or the crate's `description`
// in its Cargo.toml, and run `just caps`.
import { execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { declaredInputs, failureTables, uiDocs } from "./failure-tables.mjs";

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
const documents = read("documents.json").documents.filter((d) => d.written);
const capabilities = readdirSync(join(REGISTRY, "capabilities"))
  .filter((f) => f.endsWith(".json"))
  .map((f) => read(`capabilities/${f}`))
  .sort((a, b) => a.id.localeCompare(b.id));
const docKinds = new Set(documents.map((d) => d.kind));

const esc = (text) =>
  String(text ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

// Text from the registry holds paragraphs split at blank lines, `code`
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

const json = (value) =>
  `<div class="jt">${tree(value)}</div><details class="rawblock" id="raw"><summary>Raw JSON</summary>${raw(value)}</details>`;

// The schemas that schemars writes use objects, arrays, maps, enums,
// constants, and `oneOf` of constants or objects, and nothing else, so the
// property tree handles only those.
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

// The dark values apply under the system's dark setting, unless the root
// says data-theme="light", and under data-theme="dark".
const DARK = `--key: #7ab7ff; --str: #7ee787; --num: #ffa657; --lit: #d2a8ff; --warn: #ff8a80; --bg: #16181d; --fg: #e8e8ea; --muted: #a0a4ab; --line: #343841; --code: #20232a; --link: #7ab7ff;
  --lib: #6fcf75; --kernel: #f0a347; --cap: #79b0ff; --asm: #c59af0; --kind: #5fd0cf; --edge: #6b717b; --out: #ff8a65; --in: #a9b6d6; --panel: #1b1e24; color-scheme: dark;`;
const STYLE = `
:root { --key: #0b5cad; --str: #1a7f37; --num: #953800; --lit: #8250df; --warn: #b3261e; --bg: #fff; --fg: #1d1d1f; --muted: #5f6368; --line: #d8dadf; --code: #f3f4f6; --link: #0b5cad;
  --lib: #2e7d32; --kernel: #b45309; --cap: #1d5fbf; --asm: #7b3fb0; --kind: #0f7b7b; --edge: #9aa1ab; --out: #d14b1f; --in: #5b6b8c; --panel: #fafbfc; }
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) { ${DARK} }
}
:root[data-theme="dark"] { ${DARK} }
body { background: var(--bg); color: var(--fg); font: 16px/1.55 system-ui, sans-serif; margin: 0; }
[hidden] { display: none !important; }
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
main.wide, header.wide { max-width: 96rem; }
.lede { max-width: 42rem; }
.stats { font-variant-numeric: tabular-nums; color: var(--muted); }
.legend { display: flex; flex-wrap: wrap; gap: 6px 18px; font-size: 0.85em; color: var(--muted); margin: 8px 0; }
.legend span { display: inline-flex; align-items: center; gap: 6px; }
.legend svg { flex: none; }
figure.graph { margin: 12px 0 24px; display: grid; gap: 12px; }
.scroll { overflow-x: auto; border: 1px solid var(--line); border-radius: 8px; background: var(--panel); }
svg.g { display: block; width: 100%; height: auto; font: 11.5px/1 ui-monospace, SFMono-Regular, Menlo, monospace; }
svg.g.map { min-width: 1100px; }
svg.g.bus { width: auto; max-width: 100%; margin: 0 auto; }
.g .edge { fill: none; stroke: var(--edge); stroke-width: 1.1; opacity: 0.5; transition: opacity 0.15s; }
.g .edge.opt { stroke-dasharray: 4 3; }
.g.focus .edge { opacity: 0.05; }
.g.focus .edge.out { opacity: 1; stroke: var(--out); stroke-width: 2; }
.g.focus .edge.in { opacity: 1; stroke: var(--in); stroke-width: 2; }
.g .node { cursor: pointer; }
.g .node:focus { outline: none; }
.g .node rect.pill { fill: var(--bg); stroke: var(--c); stroke-width: 1.5; }
.g .node:focus-visible rect.pill { stroke-width: 3; }
.g .node text { fill: var(--fg); }
.g .node .mark { fill: var(--c); }
.g .node .mark.hollow { fill: var(--bg); stroke: var(--c); stroke-width: 2; }
.g.focus .node { opacity: 0.3; }
.g.focus .node.on { opacity: 1; }
.g .node.sel rect.pill { fill: var(--c); }
.g .node.sel text { fill: var(--bg); }
.g .node.sel .mark, .g .node.sel .mark.hollow { fill: var(--bg); stroke: var(--bg); }
.g .colhead { fill: var(--muted); font: 600 10.5px system-ui, sans-serif; letter-spacing: 0.08em; text-transform: uppercase; }
.g .bracket { stroke: var(--line); stroke-width: 1; fill: none; }
.library { --c: var(--lib); } .kernel { --c: var(--kernel); } .capability { --c: var(--cap); } .assembly { --c: var(--asm); } .kind { --c: var(--kind); }
.panel { border-left: 3px solid var(--line); padding: 2px 0 2px 14px; min-height: 5.5rem; }
.panel h3 { margin: 0 0 4px; display: flex; flex-wrap: wrap; align-items: baseline; gap: 8px; }
.panel p { margin: 4px 0; }
.chip { font: 600 0.7em system-ui, sans-serif; letter-spacing: 0.06em; text-transform: uppercase; color: var(--c); border: 1px solid var(--c); border-radius: 999px; padding: 1px 8px; }
.picks { display: flex; flex-wrap: wrap; gap: 4px 6px; margin-top: 2px; }
.picks button { font: 0.82em ui-monospace, monospace; background: var(--code); color: var(--fg); border: 1px solid var(--line); border-radius: 4px; padding: 1px 6px; cursor: pointer; }
.picks button:hover, .picks button:focus-visible { border-color: var(--link); }
td.dot { text-align: center; color: var(--asm); }
th.ui { font: 600 0.8em ui-monospace, monospace; text-align: center; }
.trace { margin-top: 8px; }
.trace h3 { margin: 28px 0 4px; }
.tracebar { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; margin: 10px 0; }
.tracebar button { font: 600 0.9em system-ui, sans-serif; background: var(--code); color: var(--fg); border: 1px solid var(--line); border-radius: 6px; padding: 4px 12px; cursor: pointer; }
.tracebar button:hover { border-color: var(--link); }
.tracebar button:focus-visible, .stepbtn:focus-visible { outline: 2px solid var(--link); outline-offset: 2px; }
.where { font-variant-numeric: tabular-nums; }
.traceview { display: grid; grid-template-columns: minmax(0, auto) minmax(18rem, 1fr); gap: 16px; align-items: start; }
@media (min-width: 1151px) { .traceview > .scroll { position: sticky; top: 12px; } }
@media (max-width: 1150px) { .traceview { grid-template-columns: minmax(0, 1fr); } }
svg.seq { display: block; font: 12px system-ui, sans-serif; }
.seq .lane rect { fill: var(--bg); stroke: var(--line); }
.seq .lane text { fill: var(--fg); font-weight: 600; font-size: 11.5px; }
.seq .life { stroke: var(--line); stroke-dasharray: 3 4; fill: none; }
.seq .arrow { fill: none; stroke: var(--edge); stroke-width: 1.5; }
.seq .arrow.back { stroke-dasharray: 5 4; }
.seq .tip { fill: var(--edge); }
.seq .num { fill: var(--bg); stroke: var(--edge); stroke-width: 1.5; }
.seq .numt { fill: var(--muted); font: 600 11px system-ui, sans-serif; }
.seq .label { fill: var(--muted); }
.seq .step { cursor: pointer; outline: none; }
.seq .step.done .arrow, .seq .step.done .num { stroke: var(--in); }
.seq .step.done .tip { fill: var(--in); }
.seq .step.now .arrow { stroke: var(--out); stroke-width: 2.5; }
.seq .step.now .tip { fill: var(--out); }
.seq .step.now .num { fill: var(--out); stroke: var(--out); }
.seq .step.now .numt { fill: var(--bg); }
.seq .step.now .label { fill: var(--fg); font-weight: 600; }
ol.steps { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
ol.steps li { border: 1px solid var(--line); border-radius: 8px; padding: 8px 10px; background: var(--bg); }
ol.steps li.now { border-color: var(--out); background: var(--panel); }
.stepbtn { all: unset; box-sizing: border-box; display: flex; gap: 10px; align-items: baseline; cursor: pointer; width: 100%; }
.stepbtn .n { font: 600 0.78em system-ui, sans-serif; width: 1.7em; height: 1.7em; border-radius: 999px; display: inline-grid; place-items: center; background: var(--code); color: var(--muted); flex: none; }
li.now .stepbtn .n { background: var(--out); color: var(--bg); }
.stepbtn .lanes { color: var(--muted); font-size: 0.85em; }
.stepbody { margin-left: calc(1.7em + 10px); font-size: 0.92em; }
.stepbody p { margin: 4px 0; }
li:not(.now) .stepbody > p:first-child { color: var(--muted); }
.at { color: var(--muted); line-height: 1.9; }
.at code { font-size: 0.78em; word-break: break-all; }
.bucketmap { display: grid; grid-template-columns: repeat(auto-fit, minmax(17rem, 1fr)); gap: 12px; margin: 12px 0 8px; }
.area { border: 1px solid var(--line); border-radius: 8px; padding: 12px 14px; background: var(--panel); min-width: 0; }
.area h3 { margin: 0 0 6px; display: flex; align-items: center; gap: 8px; font-size: 1.05em; }
.area p { margin: 4px 0 8px; font-size: 0.92em; }
.area ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
.area li { font-size: 0.88em; display: flex; flex-wrap: wrap; gap: 4px 6px; align-items: baseline; }
.area code.key { font-weight: 600; }
.swatch { width: 10px; height: 10px; border-radius: 2px; background: var(--c); flex: none; }
.entry { --c: var(--kind); } .pointer { --c: var(--kernel); } .blob { --c: var(--asm); } .device { --c: var(--muted); }
.files { padding: 10px 12px; }
ul.tree { list-style: none; margin: 0; padding: 0; font: 0.84em/1.5 ui-monospace, SFMono-Regular, Menlo, monospace; display: grid; gap: 2px; }
ul.tree li { display: flex; flex-wrap: wrap; gap: 2px 10px; align-items: baseline; padding: 3px 6px; border-radius: 4px; }
ul.tree li.dir { margin-top: 8px; font-weight: 600; }
ul.tree li.dir:first-child { margin-top: 0; }
ul.tree li.dir .muted { font-weight: 400; font-family: system-ui, sans-serif; }
ul.tree li.file { margin-left: 1.4em; }
ul.tree li.file .mark { width: 8px; height: 8px; border-radius: 50%; background: var(--c); flex: none; align-self: center; }
ul.tree li.file.pointer .mark { border-radius: 2px; }
ul.tree li.file.blob .mark { border-radius: 1px; transform: rotate(45deg); }
ul.tree li.file.device .mark { background: none; border: 1.5px solid var(--c); border-radius: 2px; }
ul.tree li .note { color: var(--muted); font-family: system-ui, sans-serif; }
ul.tree li.new { background: color-mix(in srgb, var(--out) 14%, transparent); }
ul.tree li.new .note { color: var(--fg); }
ul.tree li.hot { background: color-mix(in srgb, var(--in) 18%, transparent); }
ul.tree li.ghost code { text-decoration: line-through; color: var(--muted); }
ul.tree li.ghost .mark { background: none; border: 1.5px dashed var(--c); }
.word { font: 600 0.72em system-ui, sans-serif; letter-spacing: 0.04em; color: var(--out); border: 1px solid currentColor; border-radius: 999px; padding: 0 7px; margin-left: 4px; white-space: nowrap; }
@media (prefers-reduced-motion: reduce) { .g .edge { transition: none; } }
`;

function layout(title, body, { wide = false, script = "" } = {}) {
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
<header${wide ? ' class="wide"' : ""}><a href="index.html">wasm-transit</a><a href="architecture.html">Architecture</a><a href="kernel.html">Kernel ops</a></header>
<main${wide ? ' class="wide"' : ""}>
${body}
<footer>Generated from the registry. Commit ${esc((e.source_commit ?? "unknown").slice(0, 10))}, profile ${esc(e.profile ?? "unknown")}.</footer>
</main>
${script ? `<script>${script}</script>` : ""}
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
<p class="muted">Schema ${esc(d.schema)}. The store kind is ${esc(d.store_kind ?? "none (not stored)")}.</p>
<h2>Schema</h2>
<div id="schema">${schemaView(schema)}</div>
<details class="rawblock"><summary>Raw schema</summary>${raw(schema)}</details>
<h2>Example</h2>
<div id="example">${json(example)}</div>`,
  );
}

// A crate's class comes from its name, as the arch gates read it. The map
// leaves out the single-capability modules (module-<cap>), because each
// links one capability and the kernel, and the map says so in words.
const metadata = JSON.parse(
  execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps", "--offline"], {
    cwd: ROOT,
    encoding: "utf8",
    maxBuffer: 64 << 20,
  }),
);
const crateClass = (name) =>
  name === "cap-abi" ? "kernel"
  : name.startsWith("cap-") ? "capability"
  : name.startsWith("transit-") ? "library"
  : name.startsWith("module-ui-") ? "assembly"
  : null;
const CLASS_NAME = { library: "Library", kernel: "Kernel", capability: "Capability", assembly: "UI assembly", kind: "Document kind" };
const singleModules = metadata.packages.filter((p) => p.name.startsWith("module-") && !p.name.startsWith("module-ui-")).length;
const crates = metadata.packages
  .filter((p) => crateClass(p.name))
  .map((p) => ({ id: p.name, cls: crateClass(p.name), description: p.description ?? "", deps: p.dependencies }))
  .sort((a, b) => a.id.localeCompare(b.id));
const crateIds = new Set(crates.map((c) => c.id));
const crateEdges = crates.flatMap((c) =>
  c.deps.filter((d) => d.kind === null && crateIds.has(d.name)).map((d) => ({ from: c.id, to: d.name, optional: d.optional })),
);
const capPage = (crate) => {
  const id = crate.replace(/^cap-/, "");
  return capabilities.some((c) => c.id === id) ? `cap-${id}.html` : null;
};

function layers(ids, edges) {
  const out = new Map(ids.map((id) => [id, []]));
  for (const e of edges) out.get(e.from).push(e.to);
  const rank = new Map();
  const visit = (id) => {
    if (!rank.has(id)) rank.set(id, out.get(id).length ? 1 + Math.max(...out.get(id).map(visit)) : 0);
    return rank.get(id);
  };
  ids.forEach(visit);
  const columns = [];
  for (const id of ids) (columns[rank.get(id)] ??= []).push(id);
  return columns;
}

function order(columns, edges) {
  const near = new Map();
  for (const e of edges) {
    (near.get(e.from) ?? near.set(e.from, []).get(e.from)).push(e.to);
    (near.get(e.to) ?? near.set(e.to, []).get(e.to)).push(e.from);
  }
  const place = new Map();
  const mark = () => columns.forEach((col) => col.forEach((id, i) => place.set(id, col.length > 1 ? i / (col.length - 1) : 0.5)));
  mark();
  for (let sweep = 0; sweep < 8; sweep++) {
    const seq = sweep % 2 ? [...columns.keys()].reverse() : [...columns.keys()];
    for (const c of seq) {
      const score = (id) => {
        const n = (near.get(id) ?? []).filter((m) => place.has(m));
        return n.length ? n.reduce((s, m) => s + place.get(m), 0) / n.length : place.get(id);
      };
      const scores = new Map(columns[c].map((id) => [id, score(id)]));
      columns[c].sort((a, b) => scores.get(a) - scores.get(b) || a.localeCompare(b));
      mark();
    }
  }
  return columns;
}

const ROW = 30;
const PILL_H = 22;
const CHAR = 6.95;
const pillWidth = (label) => Math.round(label.length * CHAR + 30);

// A kind that the agency does not keep has a hollow marker, and an outside
// format has a square one.
function nodeSvg(n) {
  const w = pillWidth(n.label);
  const cy = n.y + PILL_H / 2;
  const mark =
    n.marker === "square" ? `<rect class="mark" x="${n.x + 6}" y="${cy - 4}" width="8" height="8" rx="1"/>`
    : `<circle class="mark${n.marker === "hollow" ? " hollow" : ""}" cx="${n.x + 11}" cy="${cy}" r="${n.marker === "hollow" ? 3.4 : 4}"/>`;
  return `<g class="node ${n.cls}" data-id="${esc(n.id)}" tabindex="0" role="button" aria-label="${esc(`${CLASS_NAME[n.cls]} ${n.label}`)}">` +
    `<rect class="pill" x="${n.x}" y="${n.y}" width="${w}" height="${PILL_H}" rx="${PILL_H / 2}"/>${mark}` +
    `<text x="${n.x + 21}" y="${cy + 4}">${esc(n.label)}</text></g>`;
}

function edgeSvg(e, nodes) {
  const a = nodes.get(e.from);
  const b = nodes.get(e.to);
  const sx = a.x;
  const sy = a.y + PILL_H / 2;
  const ex = b.x + pillWidth(b.label);
  const ey = b.y + PILL_H / 2;
  const dx = Math.max(24, (sx - ex) / 2);
  return `<path class="edge${e.optional ? " opt" : ""}" data-from="${esc(e.from)}" data-to="${esc(e.to)}" d="M${sx} ${sy} C${sx - dx} ${sy} ${ex + dx} ${ey} ${ex} ${ey}"/>`;
}

const picks = (ids, label) =>
  `<div class="picks">${ids.map((id) => `<button type="button" data-pick="${esc(id)}">${esc(label(id))}</button>`).join("") || '<span class="muted">none</span>'}</div>`;

function crateMap() {
  const columns = order(layers(crates.map((c) => c.id), crateEdges), crateEdges);
  const byId = new Map(crates.map((c) => [c.id, c]));
  const label = (id) => (byId.get(id).cls === "assembly" ? id.replace(/^module-ui-/, "") : id);
  const top = 46;
  const tallest = Math.max(...columns.map((c) => c.length));
  const nodes = new Map();
  let x = 16;
  const columnX = [];
  columns.forEach((col, c) => {
    columnX[c] = x;
    col.forEach((id, i) => {
      const crate = byId.get(id);
      nodes.set(id, { id, label: label(id), cls: crate.cls, x, y: top + ((tallest - col.length) / 2 + i) * ROW, marker: "dot" });
    });
    x += Math.max(...col.map((id) => pillWidth(label(id)))) + 46;
  });
  const width = x - 30;
  const height = top + tallest * ROW + 8;
  const groups = [];
  columns.forEach((col, c) => {
    const cls = byId.get(col[0]).cls;
    const last = groups.at(-1);
    if (last && last.cls === cls) last.to = c;
    else groups.push({ cls, from: c, to: c });
  });
  const heads = groups.map((g) => {
    const x0 = columnX[g.from];
    const x1 = columnX[g.to] + Math.max(...columns[g.to].map((id) => pillWidth(label(id))));
    const name = { library: "Libraries", kernel: "Kernel", capability: "Capabilities", assembly: "UI assemblies" }[g.cls];
    return `<text class="colhead" x="${x0}" y="18">${name}</text><path class="bracket" d="M${x0} 28 H${x1}"/>`;
  });
  const svg = `<svg class="g map" id="map" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" role="group" aria-label="The crate map: ${crates.length} crates and ${crateEdges.length} dependencies">` +
    heads.join("") +
    crateEdges.map((e) => edgeSvg(e, nodes)).join("") +
    [...nodes.values()].map(nodeSvg).join("") +
    `</svg>`;
  const usedBy = (id) => crateEdges.filter((e) => e.to === id).map((e) => e.from);
  const dependsOn = (id) => crateEdges.filter((e) => e.from === id).map((e) => e.to);
  const panels = crates.map((c) => {
    const page = c.cls === "kernel" ? "kernel.html" : c.cls === "capability" ? capPage(c.id) : null;
    const optional = crateEdges.filter((e) => e.from === c.id && e.optional).map((e) => e.to);
    return `<div class="detail panel ${c.cls}" data-for="${esc(c.id)}" hidden>
<h3><code>${esc(c.id)}</code> <span class="chip">${CLASS_NAME[c.cls]}</span></h3>
<p>${inline(c.description)}</p>
<p><strong>Depends on</strong> ${dependsOn(c.id).length}${optional.length ? ` <span class="muted">(${optional.length} optional, for the generator: ${optional.map(esc).join(", ")})</span>` : ""}</p>${picks(dependsOn(c.id), label)}
<p><strong>Used by</strong> ${usedBy(c.id).length}</p>${picks(usedBy(c.id), label)}
${page ? `<p><a href="${page}">${c.cls === "kernel" ? "The kernel ops" : "Its ops and examples"}</a></p>` : ""}
</div>`;
  });
  const rows = crates
    .map((c) => `<tr><td><code>${esc(c.id)}</code></td><td>${CLASS_NAME[c.cls]}</td><td>${dependsOn(c.id).map((d) => `<code>${esc(d)}</code>`).join(" ") || "none"}</td></tr>`)
    .join("\n");
  return { svg, panels: panels.join("\n"), rows };
}

// A kind with a schema here is the agency's (filled: kept in the bucket;
// hollow: not kept). A kind with none is an outside format that the kernel
// loads. The kind table joins a kernel load such as `load_edits` to its
// document kind by tag, so the graph names the kind as its schema page does
// (`operator_edits`). A load with no entry there keeps its own name.
function busGraph() {
  const written = read("documents.json").documents.filter((d) => d.written);
  const table = JSON.parse(readFileSync(join(ROOT, "crates/cap-abi/kinds.json"), "utf8")).kinds;
  const loadKind = new Map((pointer.kernel_ops ?? []).filter((k) => k.kind).map((k) => [k.name, k.kind]));
  const docByLoad = new Map();
  for (const d of written) {
    const op = table.find((t) => t.tag === d.tag)?.load?.op;
    docByLoad.set(op && loadKind.has(op) ? loadKind.get(op) : d.kind, d);
  }
  const kindName = (load) => docByLoad.get(load)?.kind ?? load;
  const kindInfo = new Map(written.map((d) => [d.kind, d]));
  const loadName = new Map([...docByLoad].map(([load, d]) => [d.kind, load]));
  const edges = capabilities.flatMap((c) => (c.documents ?? []).map((k) => ({ from: `cap:${c.id}`, to: `kind:${kindName(k)}` })));
  const kinds = [...new Set(edges.map((e) => e.to.slice(5)))].sort();
  const columns = order([capabilities.map((c) => `cap:${c.id}`), kinds.map((k) => `kind:${k}`)], edges);
  const capW = Math.max(...capabilities.map((c) => pillWidth(c.id)));
  const kindW = Math.max(...kinds.map((k) => pillWidth(k)));
  const gap = 230;
  const top = 46;
  const tallest = Math.max(columns[0].length, columns[1].length);
  const nodes = new Map();
  const marker = (k) => (!kindInfo.has(k) ? "square" : kindInfo.get(k).store_kind ? "dot" : "hollow");
  columns[0].forEach((id, i) => {
    const label = id.slice(4);
    nodes.set(id, { id, label, cls: "capability", x: 16 + capW - pillWidth(label), y: top + ((tallest - columns[0].length) / 2 + i) * ROW, marker: "dot" });
  });
  columns[1].forEach((id, i) => {
    const label = id.slice(5);
    nodes.set(id, { id, label, cls: "kind", x: 16 + capW + gap, y: top + ((tallest - columns[1].length) / 2 + i) * ROW, marker: marker(label) });
  });
  const width = 16 + capW + gap + kindW + 16;
  const height = top + tallest * ROW + 8;
  const path = (e) => {
    const a = nodes.get(e.from);
    const b = nodes.get(e.to);
    const sx = a.x + pillWidth(a.label);
    const sy = a.y + PILL_H / 2;
    const ex = b.x;
    const ey = b.y + PILL_H / 2;
    const dx = (ex - sx) / 2;
    return `<path class="edge" data-from="${esc(e.from)}" data-to="${esc(e.to)}" d="M${sx} ${sy} C${sx + dx} ${sy} ${ex - dx} ${ey} ${ex} ${ey}"/>`;
  };
  const svg = `<svg class="g bus" id="bus" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" role="group" aria-label="The bus: ${capabilities.length} capabilities, ${kinds.length} kinds of document, ${edges.length} reads">` +
    `<text class="colhead" x="${16 + capW}" y="18" text-anchor="end">Capabilities</text>` +
    `<text class="colhead" x="${16 + capW + gap}" y="18">Kinds of document</text>` +
    edges.map(path).join("") +
    [...nodes.values()].map(nodeSvg).join("") +
    `</svg>`;
  const readers = (k) => edges.filter((e) => e.to === `kind:${k}`).map((e) => e.from);
  const capPanels = capabilities.map((c) => `<div class="detail panel capability" data-for="cap:${esc(c.id)}" hidden>
<h3><code>${esc(c.id)}</code> <span class="chip">Capability</span></h3>
<p>${inline(firstSentence(c.purpose))}</p>
<p><strong>Reads</strong> ${(c.documents ?? []).length}</p>${picks((c.documents ?? []).map((k) => `kind:${kindName(k)}`), (id) => id.slice(5))}
<p><a href="cap-${esc(c.id)}.html">Its ops and examples</a></p>
</div>`);
  const kindPanels = kinds.map((k) => {
    const info = kindInfo.get(k);
    const kept = !info ? "An outside format. The kernel loads it, and no schema of ours describes it."
      : info.store_kind ? `The agency keeps it in the bucket (store kind <code>${esc(info.store_kind)}</code>).`
      : "Not kept in the bucket: it is derived, or kept on the device.";
    return `<div class="detail panel kind" data-for="kind:${esc(k)}" hidden>
<h3><code>${esc(k)}</code> <span class="chip">Document kind</span>${info ? ` <span class="muted">v${info.schema_version}</span>` : ""}</h3>
<p>${kept}</p>
${loadName.has(k) && loadName.get(k) !== k ? `<p class="muted">Capabilities load it as <code>${esc(loadName.get(k))}</code>.</p>` : ""}
<p><strong>Read by</strong> ${readers(k).length}</p>${picks(readers(k), (id) => id.slice(4))}
${info ? `<p><a href="doc-${esc(k)}.html">Its schema and example</a></p>` : ""}
</div>`;
  });
  const rows = capabilities
    .map((c) => `<tr><td><a href="cap-${esc(c.id)}.html">${esc(c.id)}</a></td><td>${(c.documents ?? []).map((k) => kindLink(kindName(k))).join(", ") || "none"}</td></tr>`)
    .join("\n");
  return { svg, panels: [...capPanels, ...kindPanels].join("\n"), rows, kinds: kinds.length, reads: edges.length };
}

function uiTable() {
  const uis = crates.filter((c) => c.cls === "assembly");
  const links = new Set(crateEdges.filter((e) => e.from.startsWith("module-ui-")).map((e) => `${e.from} ${e.to}`));
  const head = uis.map((u) => `<th class="ui">${esc(u.id.replace(/^module-ui-/, ""))}</th>`).join("");
  const rows = capabilities
    .map((c) => {
      const cells = uis
        .map((u) => (links.has(`${u.id} cap-${c.id}`) ? `<td class="dot" data-ui="${esc(u.id)}" data-cap="${esc(c.id)}" aria-label="linked">●</td>` : "<td></td>"))
        .join("");
      return `<tr><td><a href="cap-${esc(c.id)}.html">${esc(c.id)}</a></td>${cells}</tr>`;
    })
    .join("\n");
  const totals = uis.map((u) => `<td class="dot">${crateEdges.filter((e) => e.from === u.id && e.to.startsWith("cap-") && e.to !== "cap-abi").length}</td>`).join("");
  return `<div class="scroll"><table><tr><th>Capability</th>${head}</tr>\n${rows}\n<tr><th>Linked</th>${totals}</tr></table></div>`;
}

function inputsSection() {
  const tables = failureTables(uiDocs(ROOT));
  const every = (s) => (s == null ? "once, at boot" : `every ${s} s`);
  const keeps = (k) => (k === "none" ? "nothing (it never shows an old answer as live)" : "its last copy, labelled with its age");
  return Object.entries(declaredInputs(ROOT))
    .map(([ui, inputs]) => {
      const rows = new Map((tables[ui]?.rows ?? []).map((r) => [r.name, r]));
      const body = inputs
        .map((i) => {
          const fails = (i.fails_as ?? [])
            .map((name) => {
              const row = rows.get(name);
              const shows = row ? inline(row.cells[2] ?? "") : "";
              const catches = row ? inline(row.cells[5] ?? "") : "";
              return `<li><strong>${esc(name)}</strong>${shows ? `: still shows ${shows}` : ""}${catches ? `; catches up: ${catches}` : ""}</li>`;
            })
            .join("");
          return `<tr data-ui="${esc(ui)}" data-input="${esc(i.id)}"><td><strong>${esc(i.name)}</strong><br><span class="muted">${inline(i.from)}</span></td><td>${every(i.every_s)}</td><td>${esc(String(i.limit_s))} s</td><td>${keeps(i.keeps)}${i.old_after_s ? `; old after ${esc(String(i.old_after_s))} s` : ""}</td><td><ul>${fails}</ul></td></tr>`;
        })
        .join("\n");
      return `<h3 id="inputs-${esc(ui)}">${esc(ui)}</h3>
<div class="scroll"><table><tr><th>Input</th><th>Read</th><th>Time limit</th><th>When a read fails, the page keeps</th><th>Its failure table's rows</th></tr>
${body}
</table></div>`;
    })
    .join("\n");
}

// Each step names the code it describes (`at`: a file and a text in it).
// docs-site.test.mjs fails when a file or its text is gone, so a rename
// breaks the build until the trace follows it.
//
// A step goes from one lane to another (`from`, `to`), stays in its
// lane (`self`), or carries an answer back (`back`).
const TRACES = [
  {
    id: "request",
    title: "One request: a stop display asks for its departures",
    lede: "Every answer a page shows comes this way. The answer comes from the page's module: one WebAssembly file, built for this UI from the capabilities it links (here, module-ui-stop-display). The page asks with documents, and the module answers from the documents it holds. No step on the way makes a domain decision except the library at the end.",
    lanes: ["Stop display page", "Engine", "Bus worker", "Kernel", "board capability", "transit-dispatch"],
    steps: [
      { from: 0, to: 1, title: "The page asks", text: "The page asks the board capability for the departures at one stop, now, in the agency's time zone. It names the network, today's edits, and the live feed as documents.", at: [["web/src/uis/stop-display/host.svelte.ts", 'ask("board", "grouped_board"']] },
      { from: 1, self: true, title: "Documents become hashes", text: "The engine sends each document to the module once and names it by its content hash after that. The same question over the same documents answers from the engine's cache.", at: [["web/src/engine.ts", "async function asked("], ["web/src/engine.ts", "(await register(cap, value)).hash"]] },
      { from: 1, to: 2, title: "Off the render thread", text: "The request goes to the page's one bus worker as JSON text. The worker checked the module's bytes against their pinned hash, and runs it with no imports: the module cannot reach the network, the clock, or the page.", at: [["web/src/bus.worker.ts", "self.onmessage"], ["web/src/bus.worker.ts", "WebAssembly.instantiate(module, {})"]] },
      { from: 2, to: 3, title: "Into the module", text: "Every module has the same flat exports. The worker writes the text into the module's memory and calls transit_invoke.", at: [["crates/cap-abi/src/lib.rs", 'pub unsafe extern "C" fn transit_invoke(']] },
      { from: 3, to: 4, title: "The kernel routes", text: "A load registers a document. Any other request names its capability, and the kernel hands it to that capability in this assembly.", at: [["crates/cap-abi/src/kernel.rs", "pub fn invoke(caps: &[&Cap], text: &str)"]] },
      { from: 4, self: true, title: "Documents by hash", text: "The request carries only hashes. The capability looks up the network, the edits, and the live day by those hashes, among the documents the engine sent the module before. A hash the module does not hold is an error, never a guess.", at: [["crates/cap-board/src/lib.rs", "kind::<transit_network::Network>().require(&network_hash)"]] },
      { from: 4, to: 5, title: "The library answers", text: "transit-dispatch answers the question: the day's trips at that stop, patched by today's edits and the live feed, grouped by where they go.", at: [["crates/transit-dispatch/src/lib.rs", "pub fn grouped_board_at("]] },
      { from: 5, to: 0, back: true, title: "The answer comes back", text: "The answer goes back as JSON through the same steps. The page shows it as given. It computes nothing of its own.", at: [["web/src/uis/stop-display/host.svelte.ts", "display.board = board;"]] },
    ],
  },
  {
    id: "write",
    title: "One write: a dispatcher saves a note",
    lede: "Every write to the agency's record comes this way. The page sends what the person asked for, and the server's receive path checks it against what the person saw. The bucket keeps every entry.",
    lanes: ["Disruptions page", "Outbox", "Server", "notes capability", "transit-log", "Bucket"],
    steps: [
      { from: 0, to: 1, title: "The person saves", text: "The page writes the intent: the ops (write this note), the log's seq the person read, and an idempotency key. It never names who wrote the note or when.", at: [["web/src/uis/disruptions/host.svelte.ts", 'queueWrite({ log: "notes"']] },
      { from: 1, to: 2, title: "The outbox sends it", text: "The write waits on the device until the server answers. With no signal it keeps waiting, and every attempt sends the same key, so a write lands once.", at: [["web/src/outbox.ts", "export class Outbox"], ["server/main.go", 'mux.HandleFunc("/logs/"']] },
      { from: 2, self: true, title: "The server stamps it", text: "The server writes the entry's header: the writer it verified, its own clock, and the next seq. It finds the entries the person did not read, since the seq they read.", at: [["server/logs/receive.go", "func (ls *Logs) Receive("], ["server/logs/receive.go", "ls.store.Entries(f.name, max(*w.ReadSeq, 0), f.seq)"]] },
      { from: 2, to: 3, title: "The capability runs the ops", text: "The notes capability runs the ops over the documents at the head. It refuses only what makes the data wrong.", at: [["crates/cap-notes/src/lib.rs", "Request::Receive { write, heads, ops }"]] },
      { from: 3, to: 4, title: "The log makes the entry", text: "transit-log turns the changed records into one entry, with each record's hash as stored before. A record that a write the person did not read changed is a conflict for a person to settle.", at: [["crates/transit-log/src/receive.rs", "pub fn receive("]] },
      { from: 2, to: 5, title: "The bucket keeps it", text: "The server stores the entry under its seq with a conditional create. Nothing in the bucket is changed or deleted.", at: [["server/store/log.go", "func (s *Store) AppendEntry("]] },
      { from: 5, to: 2, back: true, title: "Another write took the seq", text: "If another copy of the server took the seq first, the create fails. The server folds the new head and asks again, up to five times.", at: [["server/logs/receive.go", "for attempt := 0; attempt < 5; attempt++"]] },
      { from: 2, to: 0, back: true, title: "The page reads the new document", text: "The new head names a new notes document. The page asks for it by its hash, like any other answer.", at: [["web/src/uis/disruptions/host.svelte.ts", 'ask("notes", "between"']] },
    ],
  },
];

function traceSection(t) {
  const laneW = 132;
  const left = 16;
  const top = 58;
  const rowH = 46;
  const width = left * 2 + laneW * t.lanes.length;
  const height = top + rowH * t.steps.length + 18;
  const cx = (lane) => left + laneW * lane + laneW / 2;
  const lanes = t.lanes
    .map((name, i) => {
      const words = name.split(" ");
      const lines = words.length > 1 ? [words.slice(0, Math.ceil(words.length / 2)).join(" "), words.slice(Math.ceil(words.length / 2)).join(" ")] : [name];
      return `<g class="lane"><rect x="${cx(i) - laneW / 2 + 6}" y="6" width="${laneW - 12}" height="38" rx="6"/>` +
        lines.map((l, j) => `<text x="${cx(i)}" y="${lines.length > 1 ? 21 + j * 14 : 29}" text-anchor="middle">${esc(l)}</text>`).join("") +
        `<path class="life" d="M${cx(i)} 44 V${height - 8}"/></g>`;
    })
    .join("");
  const steps = t.steps
    .map((s, i) => {
      const y = top + rowH * i + rowH / 2;
      const n = i + 1;
      let shape;
      if (s.self) {
        const x = cx(s.from);
        shape = `<path class="arrow" d="M${x} ${y - 10} h34 v20 h-26"/><path class="tip" d="M${x + 12} ${y + 10} l8 -4.5 v9 z"/>` +
          `<circle class="num" cx="${x}" cy="${y}" r="10"/><text class="numt" x="${x}" y="${y + 4}" text-anchor="middle">${n}</text>`;
      } else {
        const x0 = cx(s.from);
        const x1 = cx(s.to);
        const dir = Math.sign(x1 - x0);
        const tip = x1 - dir * 3;
        shape = `<path class="arrow${s.back ? " back" : ""}" d="M${x0 + dir * 12} ${y} H${tip - dir * 8}"/><path class="tip" d="M${tip} ${y} l${-dir * 9} -4.5 v9 z"/>` +
          `<circle class="num" cx="${x0}" cy="${y}" r="10"/><text class="numt" x="${x0}" y="${y + 4}" text-anchor="middle">${n}</text>`;
      }
      // A label starts after the step's number on a self step or a step
      // to the right, and sits over the middle of a step back.
      const forward = !s.self && cx(s.to) > cx(s.from);
      const labelX = s.self ? cx(s.from) + 42 : forward ? cx(s.from) + 16 : (cx(s.from) + cx(s.to)) / 2;
      const anchor = s.self || forward ? "start" : "middle";
      return `<g class="step" data-step="${n}" tabindex="-1">${shape}<text class="label" x="${labelX}" y="${y - (s.self ? -4 : 7)}" text-anchor="${anchor}">${esc(s.title)}</text></g>`;
    })
    .join("");
  const svg = `<svg class="seq" id="seq-${t.id}" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" role="img" aria-label="${esc(t.title)}, in ${t.steps.length} steps">` +
    lanes + steps + `</svg>`;
  const items = t.steps
    .map(
      (s, i) => `<li data-step="${i + 1}"><button type="button" class="stepbtn"><span class="n">${i + 1}</span><span><strong>${esc(s.title)}</strong> <span class="lanes">${esc(t.lanes[s.from])}${s.self ? "" : ` → ${esc(t.lanes[s.to])}`}</span></span></button>
<div class="stepbody"><p>${inline(s.text)}</p><p class="at">${s.at.map(([file, text]) => `<code data-file="${esc(file)}">${esc(file)}</code> <code class="sym">${esc(text)}</code>`).join("<br>")}</p></div></li>`,
    )
    .join("\n");
  return `<section class="trace" id="trace-${t.id}" data-steps="${t.steps.length}">
<h3>${esc(t.title)}</h3>
<p class="lede">${esc(t.lede)}</p>
<div class="tracebar"><button type="button" class="prev" aria-label="Previous step">←</button><button type="button" class="play">Play</button><button type="button" class="next" aria-label="Next step">→</button><span class="where muted" aria-live="polite">Step 1 of ${t.steps.length}</span></div>
<div class="traceview"><div class="scroll">${svg}</div><ol class="steps">${items}</ol></div>
</section>`;
}

const TRACE_SCRIPT = `
for (const sec of document.querySelectorAll("section.trace:not(.bucket)")) {
  const total = Number(sec.dataset.steps);
  const svg = sec.querySelector("svg");
  const items = [...sec.querySelectorAll("ol.steps li")];
  const where = sec.querySelector(".where");
  const play = sec.querySelector(".play");
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  let at = 1;
  let timer = null;
  const go = (n) => {
    at = Math.min(total, Math.max(1, n));
    for (const g of svg.querySelectorAll(".step")) {
      const k = Number(g.dataset.step);
      g.classList.toggle("now", k === at);
      g.classList.toggle("done", k < at);
    }
    for (const li of items) li.classList.toggle("now", Number(li.dataset.step) === at);
    where.textContent = "Step " + at + " of " + total;
  };
  const stop = () => { clearInterval(timer); timer = null; play.textContent = "Play"; };
  sec.querySelector(".prev").addEventListener("click", () => { stop(); go(at - 1); });
  sec.querySelector(".next").addEventListener("click", () => { stop(); go(at + 1); });
  play.addEventListener("click", () => {
    if (timer) return stop();
    if (at === total) go(1);
    play.textContent = "Pause";
    timer = setInterval(() => (at < total ? go(at + 1) : stop()), still ? 4000 : 2200);
  });
  for (const li of items) li.querySelector(".stepbtn").addEventListener("click", () => { stop(); go(Number(li.dataset.step)); });
  for (const g of svg.querySelectorAll(".step")) g.addEventListener("click", () => { stop(); go(Number(g.dataset.step)); });
  sec.addEventListener("keydown", (ev) => {
    if (ev.target.closest("input, textarea")) return;
    if (ev.key === "ArrowRight") { stop(); go(at + 1); }
    if (ev.key === "ArrowLeft") { stop(); go(at - 1); }
  });
  go(1);
}
`;

// The bucket stories are written by hand like the traces. Each row says the
// step it appears at (`at`) or the one step it shows at (`only`), and each
// step names its code (`code`), which the test checks.
function bucketMap() {
  const table = JSON.parse(readFileSync(join(ROOT, "crates/cap-abi/kinds.json"), "utf8")).kinds;
  const kindChip = (k) => `<code data-kind="${esc(k.store_kind)}">${esc(k.store_kind)}</code>`;
  const logs = new Map();
  for (const k of table.filter((k) => k.log)) (logs.get(k.log) ?? logs.set(k.log, []).get(k.log)).push(k);
  const histories = new Map();
  for (const k of table.filter((k) => !k.log)) {
    for (const p of k.pointers ?? []) (histories.get(p) ?? histories.set(p, []).get(p)).push(k);
  }
  const older = table.filter((k) => k.log && (k.pointers ?? []).length);
  const ownKeys = table.filter((k) => !k.log && !(k.pointers ?? []).length);
  const rows = (map, prefix, suffix) =>
    [...map].sort(([a], [b]) => a.localeCompare(b))
      .map(([name, kinds]) => `<li><code class="key">${prefix}${esc(name)}${suffix}</code> ${kinds.map(kindChip).join(" ")}</li>`)
      .join("");
  return `<div class="bucketmap">
<section class="area entry"><h3><span class="swatch"></span>Logs</h3><p>Kept as a log of changes: one file for each change, never edited. The current document is what the log adds up to.</p><ul>${rows(logs, "logs/", "/")}</ul></section>
<section class="area pointer"><h3><span class="swatch"></span>Pointer histories</h3><p>Kept whole. Each new version is a blob, and a numbered pointer record says which blob is current. Every earlier record stays.</p><ul>${rows(histories, "pointers/", "/")}</ul>
${older.length ? `<p class="muted">Older history, read only: ${older.map((k) => `${k.pointers.map((p) => `<code>pointers/${esc(p)}/</code>`).join(" ")} (${esc(k.store_kind)}, from before its log)`).join("; ")}.</p>` : ""}</section>
<section class="area blob"><h3><span class="swatch"></span>Blobs and their own keys</h3><p>Every version a pointer names is a blob under <code class="key">blobs/</code>, named by the fingerprint of its bytes. These kinds are kept at keys of their own, with no pointer and no log:</p><ul><li>${ownKeys.map(kindChip).join(" ")}</li></ul><p class="muted">The keys of each are in <code>docs/FORMATS.md</code>.</p></section>
</div>`;
}

const NOTE_ROWS = [
  { dir: "blobs/", note: "files named by the fingerprint of their bytes" },
  { name: "9f2c51…e104", note: "the manifest, version 16", at: 1, area: "blob" },
  { name: "4b7a91…02c3", note: "the manifest, version 17", at: 5, area: "blob" },
  { dir: "pointers/manifest/", note: "which manifest is current, and every one before it" },
  { name: "00000000000000000016.json", note: "current: 9f2c51…", at: 1, area: "pointer" },
  { name: "00000000000000000017.json", note: "from 9f2c51… to 4b7a91…", at: 5, area: "pointer" },
  { dir: "logs/notes/entries/", note: "the notes log: one file for each change" },
  { name: "…01.json to …41.json", note: "41 earlier entries", at: 1, area: "entry", id: "earlier" },
  { name: "00000000000000000042.json", note: "the new note", at: 2, area: "entry", id: "e42" },
  { name: "00000000000000000043.json", note: "server A: refused, the file exists", only: 4, area: "entry", ghost: true },
  { name: "00000000000000000043.json", note: "server B's note", at: 4, area: "entry" },
  { name: "00000000000000000044.json", note: "server A's note, after it read 43", at: 4, area: "entry", id: "e44" },
  { name: "00000000000000000045.json", note: "reverts entry 44", at: 6, area: "entry" },
  { dir: "logs/notes/checkpoints/", note: "the log summed up, every 256 entries" },
  { name: "00000000000000000256.json", note: "written after entry 256", at: 7, area: "entry", ghost: true },
];

const NOTE_STEPS = [
  { title: "The bucket", word: null, text: "The agency's record is one bucket of files. A file is written once. Nothing changes it, and the server never deletes it. Three kinds of file hold everything: log entries, blobs, and pointer records. The names here are examples.", code: [["server/store/backend.go", "Create(ctx context.Context, key string, data []byte, meta map[string]string) error"]] },
  { title: "A dispatcher saves a note", word: "entry", text: "The server writes the change as the next file in the notes log, entry 42. The file holds the note, who wrote it, and when. One change, with its number in the log, is an entry.", hot: ["e42"], code: [["server/store/log.go", "func entryKey("], ["server/store/log.go", "func (s *Store) AppendEntry("]] },
  { title: "The log adds up to today's notes", word: "fold", text: "To know the notes now, the server reads entries 1 to 42 in order and keeps the result in memory. Reading a log from the start is the fold. The notes document is not a file in the bucket. It is what the log adds up to, named by its fingerprint.", hot: ["earlier", "e42"], code: [["server/logs/read.go", "func (ls *Logs) Doc("]] },
  { title: "Two servers reach for entry 43", word: "create only if absent", text: "Two copies of the server save a note at the same moment, and both try to create entry 43. The bucket creates a file only if no file has that name, so one wins. The other reads entry 43, checks its own write again, and creates entry 44. No write is lost, and none is written twice.", code: [["server/store/log.go", "if seq != l.head.Seq+1"], ["server/logs/receive.go", "for attempt := 0; attempt < 5; attempt++"]] },
  { title: "The manifest moves", word: "blob, pointer", text: "The manifest tells every page which documents are current, by fingerprint. When a page next asks for it, the server sees that the notes changed. It stores the new manifest as a blob: a file named by the fingerprint of its bytes. Then it adds pointer record 17, which says the current manifest is now 4b7a91…. Record 16 stays, so every earlier manifest can still be found.", code: [["server/main.go", 's.store.CAS("manifest", from, body, env, "", "server", time.Now().UnixMilli())'], ["server/store/store.go", 's.putBlob(data, env, "")']] },
  { title: "A mistake is undone", word: "revert", text: "Say entry 44 was a mistake. Nobody edits or deletes entry 44. A new entry, 45, puts back each record that 44 changed, as it was before. A note that 44 created leaves the current notes. Both entries stay, so anyone can see what happened, and when.", code: [["crates/transit-log/src/lib.rs", "pub fn revert("], ["server/logs/routes.go", "func (ls *Logs) handleLogRevertOf("]] },
  { title: "A summary every 256 entries", word: "checkpoint", text: "So that a server does not read thousands of entries when it starts, it writes a checkpoint after every 256 entries: the fold up to that entry. A checkpoint is a summary. The entries stay the record.", code: [["server/logs/fold.go", "const CheckpointEvery = 256"]] },
];

const FEED_ROWS = [
  { dir: "blobs/", note: "files named by the fingerprint of their bytes" },
  { name: "3f80d2…a2d1", note: "last month's GTFS zip", at: 1, area: "blob" },
  { name: "52aa17…9c07", note: "last month's network", at: 1, area: "blob" },
  { name: "a1c4e0…77e0", note: "the new GTFS zip", at: 2, area: "blob" },
  { name: "7d0e3b…1b5f", note: "the new network", at: 3, area: "blob", id: "net" },
  { name: "c58b92…40aa", note: "the new boundary", at: 3, area: "blob" },
  { name: "e9126f…6d38", note: "the derivation record", at: 4, area: "blob" },
  { dir: "pointers/source.feed/", note: "which zip the server last read" },
  { name: "00000000000000000011.json", note: "current: 3f80d2…", at: 1, area: "pointer" },
  { name: "00000000000000000012.json", note: "from 3f80d2… to a1c4e0…, writer import:feed", at: 2, area: "pointer", id: "src12" },
  { dir: "pointers/network/", note: "which network is current, and every one before it" },
  { name: "00000000000000000011.json", note: "current: 52aa17…", at: 1, area: "pointer", id: "n11" },
  { name: "00000000000000000012.json", note: "from 52aa17… to 7d0e3b…", at: 3, area: "pointer", id: "n12" },
  { name: "00000000000000000013.json", note: "server B: not written, 7d0e3b… is already current", only: 6, area: "pointer", ghost: true },
  { dir: "pointers/boundary/", note: "the feed's own numbering, beside the network" },
  { name: "00000000000000000011.json", note: "current: the old boundary", at: 1, area: "pointer" },
  { name: "00000000000000000012.json", note: "to c58b92…", at: 3, area: "pointer" },
  { dir: "pointers/derived.network/", note: "what made the network, from what" },
  { name: "00000000000000000011.json", note: "names last month's zip and network", at: 1, area: "pointer" },
  { name: "00000000000000000012.json", note: "names the new zip, network, and boundary", at: 4, area: "pointer", id: "d12" },
];

const FEED_STEPS = [
  { title: "Documents kept whole", word: null, text: "Some documents are not collections of records. The agency's GTFS zip, and the network and streets the server makes from its inputs, are replaced whole, and rarely. Each version is one blob, and a pointer history says which version is current. The names here are examples.", code: [["server/store/store.go", "func (s *Store) CAS("]] },
  { title: "The agency publishes a new feed", word: "pointer record", text: "At start, the server reads the agency's GTFS zip. Its bytes changed, so the server stores the zip as a blob and adds record 12 to its pointer history. The record names the old zip, the new one, the writer, and the time. A pointer record is one move of the pointer, never edited.", hot: ["src12"], code: [["server/source.go", "func (s *source) commit()"], ["server/source.go", 's.st.CAS(s.pointer, from, s.body, env, "", "import:"+s.raw, time.Now().UnixMilli())']] },
  { title: "The server imports the feed's network", word: "derived", text: "The network capability imports the zip and answers the network and the boundary. The server stores each one as a blob, and moves each one's pointer. These documents are derived: made from an input, by a module.", hot: ["net", "n12"], code: [["server/derived.go", "func commitDerivation("], ["server/derived.go", 'st.CommitHead(doc.tag, doc.body, env, doc.hash, "server")']] },
  { title: "It writes down how", word: "derivation", text: "Last, the server stores a derivation record: which zip went in, which module ran, and which documents came out. It is written last on purpose. If the server stops before this, the next start makes the same documents again, with the same bytes.", hot: ["d12"], code: [["server/derived.go", 'st.CommitHead(pointer, body, env, "", "server")']] },
  { title: "The next start does no work", word: null, text: "When a server starts with the same zip, it reads the derivation record, sees the same inputs, and serves the stored network. It runs no import. An import costs C-TRAN about 5 GB of memory at its peak, so this matters.", hot: ["d12"], code: [["server/derived.go", "func storedDerivation("]] },
  { title: "Two servers make the same network", word: null, text: "Two copies of the server start together, and both make the network from the same zip. The bytes are the same, so the fingerprint is the same. The second finds that network already current, and writes no record.", code: [["server/store/store.go", "if p.seq > 0 && p.to == hash {"]] },
  { title: "Every version can still be read", word: "pointer history", text: "Nothing in a pointer history is deleted. Its records list each version, who made it current, and when. An auditor can read the exact network a pick was built from. To go back, an operator adds a new record that names an older version, and the move is kept too.", hot: ["n11", "n12"], code: [["server/pointers.go", "func (s *server) handlePointerLog("], ["server/cmd/store/main.go", "store revert [-writer W] [-wasm server.wasm] [-kinds kinds.json] <url> <pointer> <hash>"]] },
];

const PLAN_ROWS = [
  { dir: "on the planner's device", note: "the browser's own storage, not the bucket" },
  { name: "draft: spring-2027", note: "a fork of the running pick, edited", at: 1, area: "device", id: "draft" },
  { dir: "blobs/", note: "files named by the fingerprint of their bytes" },
  { name: "52aa17…9c07", note: "the running pick's network", at: 1, area: "blob" },
  { name: "b3e9d1…5a20", note: "the spring-2027 network, frozen", at: 2, area: "blob", id: "net" },
  { name: "4b7a91…02c3", note: "the manifest that names the new pick", at: 4, area: "blob" },
  { dir: "pointers/plan.spring-2027/", note: "every network this plan froze" },
  { name: "00000000000000000001.json", note: "frozen for public comment", at: 1, area: "pointer", id: "p1" },
  { name: "00000000000000000002.json", note: "frozen to adopt: b3e9d1…", at: 2, area: "pointer", id: "p2" },
  { dir: "logs/plans/entries/", note: "the plans log: freezes, adoptions, retirements" },
  { name: "…01.json to …36.json", note: "36 earlier entries", at: 1, area: "entry" },
  { name: "00000000000000000037.json", note: "freezes b3e9d1… and adopts it from 2027-03-01", at: 3, area: "entry", id: "e37" },
  { dir: "pointers/manifest/", note: "which manifest is current" },
  { name: "00000000000000000017.json", note: "names the new pick", at: 4, area: "pointer" },
];

const PLAN_STEPS = [
  { title: "The draft is on the planner's device", word: "draft", text: "A planner changes service in a draft: a fork of a published pick. The draft is kept in the browser on their device, not in the bucket. Each edit asks the author capability for the next network, and the draft keeps it whole.", hot: ["draft"], code: [["crates/transit-network/src/edit.rs", "pub fn apply_edits("], ["web/src/plans.svelte.ts", "idbSet(PLANS_KEY, set)"]] },
  { title: "Publishing freezes the network", word: "freeze", text: "The planner publishes. The server first stores the draft's network as a blob, and makes it the head of the plan's own pointer history, pointers/plan.spring-2027/. Record 1, frozen earlier for public comment, stays. A frozen network never changes.", hot: ["p1"], code: [["crates/cap-plans/src/lib.rs", 'pointer: Some(format!("plan.{}", plan.id))'], ["server/logs/receive.go", "ls.store.CommitHead(d.Pointer"]] },
  { title: "The plans log names it", word: "adopt", text: "Then one entry in the plans log records the freeze and the adoption together: the frozen version, named by its fingerprint, and the pick that runs it from 2027-03-01. A log entry names a document kept whole by its fingerprint, and never copies it.", hot: ["net", "p2"], code: [["server/plans/plans.go", '"cap": "plans", "op": "pick"']] },
  { title: "Pages see the new pick", word: null, text: "When a page next asks for the manifest, it names the new published plan set. A page reads the pick in force on each date, and the network that pick names, by its fingerprint.", hot: ["e37"], code: [["server/main.go", 's.store.CAS("manifest", from, body, env, "", "server", time.Now().UnixMilli())']] },
  { title: "Corrected by publishing again", word: null, text: "The plans log has no revert. A wrong pick is published again, or retired before it takes effect, and both are new entries. So every proposal and every network that was ever in force stays readable.", hot: ["p1", "p2", "e37"], code: [["server/logs/routes.go", "the plans log has no revert"]] },
];

const BUCKET_STORIES = [
  { id: "story-note", title: "One note in the bucket", lede: "A collection of records that people change one at a time is kept as a log. The write trace above ends here. Each step adds files, and names the one new word it needs.", rows: NOTE_ROWS, steps: NOTE_STEPS },
  { id: "story-feed", title: "One new feed in the bucket", lede: "A document that is replaced whole, and rarely, is kept as a pointer history instead. This story follows a new GTFS zip from the agency. The network it makes is the deployment's feed network. Planners author networks too: the next story follows one.", rows: FEED_ROWS, steps: FEED_STEPS },
  { id: "story-plan", title: "A planner publishes a service change", lede: "A network a planner authors is a draft on their device until it is published. Publishing keeps it whole, in its plan's pointer history, and the plans log names it by its fingerprint. This is where the two ways of keeping meet.", rows: PLAN_ROWS, steps: PLAN_STEPS },
];

function bucketStory(story) {
  const rows = story.rows.map((r) =>
    r.dir
      ? `<li class="dir"><code>${esc(r.dir)}</code> <span class="muted">${esc(r.note)}</span></li>`
      : `<li class="file ${r.area}${r.ghost ? " ghost" : ""}"${r.at ? ` data-at="${r.at}"` : ""}${r.only ? ` data-only="${r.only}"` : ""}${r.id ? ` data-row="${r.id}"` : ""}><span class="mark"></span><code>${esc(r.name)}</code> <span class="note">${esc(r.note)}</span></li>`,
  ).join("\n");
  const items = story.steps.map(
    (s, i) => `<li data-step="${i + 1}"${s.hot ? ` data-hot="${s.hot.join(" ")}"` : ""}><button type="button" class="stepbtn"><span class="n">${i + 1}</span><span><strong>${esc(s.title)}</strong>${s.word ? ` <span class="word">${esc(s.word)}</span>` : ""}</span></button>
<div class="stepbody"><p>${inline(s.text)}</p><p class="at">${s.code.map(([file, text]) => `<code data-file="${esc(file)}">${esc(file)}</code> <code class="sym">${esc(text)}</code>`).join("<br>")}</p></div></li>`,
  ).join("\n");
  return `<section class="trace bucket" id="${story.id}" data-steps="${story.steps.length}">
<h3>${esc(story.title)}</h3>
<p class="lede">${esc(story.lede)}</p>
<div class="tracebar"><button type="button" class="prev" aria-label="Previous step">←</button><button type="button" class="play">Play</button><button type="button" class="next" aria-label="Next step">→</button><span class="where muted" aria-live="polite">Step 1 of ${story.steps.length}</span></div>
<div class="traceview"><div class="scroll files"><ul class="tree">${rows}</ul></div><ol class="steps">${items}</ol></div>
</section>`;
}

// A row shows from its step on (`data-at`) or at its one step
// (`data-only`), and is lit at its step and at a step that names it
// (`data-hot`).
const BUCKET_SCRIPT = `
for (const sec of document.querySelectorAll("section.bucket")) {
  const total = Number(sec.dataset.steps);
  const rows = [...sec.querySelectorAll("li.file")];
  const items = [...sec.querySelectorAll("ol.steps li")];
  const where = sec.querySelector(".where");
  const play = sec.querySelector(".play");
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  let at = 1;
  let timer = null;
  const go = (n) => {
    at = Math.min(total, Math.max(1, n));
    const hot = (items[at - 1].dataset.hot || "").split(" ").filter(Boolean);
    for (const r of rows) {
      const from = Number(r.dataset.at || 0);
      const only = Number(r.dataset.only || 0);
      const shown = only ? only === at : from <= at;
      r.hidden = !shown;
      r.classList.toggle("new", shown && (from === at || only === at));
      r.classList.toggle("hot", shown && hot.includes(r.dataset.row));
    }
    for (const li of items) li.classList.toggle("now", Number(li.dataset.step) === at);
    where.textContent = "Step " + at + " of " + total;
  };
  const stop = () => { clearInterval(timer); timer = null; play.textContent = "Play"; };
  sec.querySelector(".prev").addEventListener("click", () => { stop(); go(at - 1); });
  sec.querySelector(".next").addEventListener("click", () => { stop(); go(at + 1); });
  play.addEventListener("click", () => {
    if (timer) return stop();
    if (at === total) go(1);
    play.textContent = "Pause";
    timer = setInterval(() => (at < total ? go(at + 1) : stop()), still ? 5000 : 3200);
  });
  for (const li of items) li.querySelector(".stepbtn").addEventListener("click", () => { stop(); go(Number(li.dataset.step)); });
  sec.addEventListener("keydown", (ev) => {
    if (ev.key === "ArrowRight") { stop(); go(at + 1); }
    if (ev.key === "ArrowLeft") { stop(); go(at - 1); }
  });
  go(1);
}
`;

const GRAPH_SCRIPT = `
for (const fig of document.querySelectorAll("figure.graph")) {
  const svg = fig.querySelector("svg");
  const edges = [...svg.querySelectorAll(".edge")];
  const nodes = new Map([...svg.querySelectorAll(".node")].map((n) => [n.dataset.id, n]));
  const panels = [...fig.querySelectorAll(".detail")];
  const empty = fig.querySelector(".detail-empty");
  let pinned = null;
  const show = (id) => {
    svg.classList.toggle("focus", !!id);
    for (const n of nodes.values()) n.classList.remove("on", "sel");
    for (const e of edges) {
      const out = !!id && e.dataset.from === id;
      const into = !!id && e.dataset.to === id;
      e.classList.toggle("out", out);
      e.classList.toggle("in", into);
      if (out || into) {
        nodes.get(e.dataset.from)?.classList.add("on");
        nodes.get(e.dataset.to)?.classList.add("on");
      }
    }
    if (id) nodes.get(id)?.classList.add("on", "sel");
  };
  const pin = (id) => {
    pinned = id;
    show(pinned);
    for (const p of panels) p.hidden = p.dataset.for !== pinned;
    empty.hidden = !!pinned;
  };
  for (const [id, n] of nodes) {
    n.addEventListener("mouseenter", () => show(id));
    n.addEventListener("mouseleave", () => show(pinned));
    n.addEventListener("focus", () => show(id));
    n.addEventListener("blur", () => show(pinned));
    n.addEventListener("click", () => pin(pinned === id ? null : id));
    n.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); pin(pinned === id ? null : id); }
      if (ev.key === "Escape") pin(null);
    });
  }
  fig.addEventListener("click", (ev) => {
    const pick = ev.target.closest("[data-pick]");
    if (pick) pin(pick.dataset.pick);
  });
}
`;

const swatch = (cls, marker = "dot") =>
  `<svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true" class="${cls}" style="color: var(--c)">` +
  (marker === "square" ? `<rect x="3" y="3" width="8" height="8" rx="1" fill="currentColor"/>`
    : marker === "hollow" ? `<circle cx="7" cy="7" r="3.6" fill="none" stroke="currentColor" stroke-width="2"/>`
    : `<circle cx="7" cy="7" r="4.2" fill="currentColor"/>`) + `</svg>`;
const line = (color, dashed = false) =>
  `<svg width="26" height="10" viewBox="0 0 26 10" aria-hidden="true"><path d="M1 5 H25" stroke="var(${color})" stroke-width="2"${dashed ? ' stroke-dasharray="4 3"' : ""}/></svg>`;

function architecturePage() {
  const map = crateMap();
  const bus = busGraph();
  const optional = crateEdges.filter((e) => e.optional).length;
  const count = (cls) => crates.filter((c) => c.cls === cls).length;
  return layout(
    "Architecture",
    `<h1>Architecture</h1>
<p class="lede">These pictures are drawn from the code each time the site is built, so they show the repo as it is. The crate map comes from Cargo's view of the workspace, and the bus and the UIs come from the registry. The two traces are written by hand, and the build checks that the code each step names still exists.</p>

<h2 id="crate-map">The crate map</h2>
<p class="lede">Each column depends only on columns to its left. The libraries hold the domain, and the network is near the root. The kernel is the seam that every module links. Each capability answers one need over its libraries, and no capability depends on another. Each generated UI links the capabilities it needs into one module. Select a crate to see what it depends on and what uses it.</p>
<p class="stats">${count("library")} libraries · 1 kernel · ${count("capability")} capabilities · ${count("assembly")} UI assemblies · ${crateEdges.length} dependencies, ${optional} of them optional. Each capability is also built alone, as its own module (${singleModules}).</p>
<div class="legend"><span>${swatch("library")}Library</span><span>${swatch("kernel")}Kernel</span><span>${swatch("capability")}Capability</span><span>${swatch("assembly")}UI assembly</span><span>${line("--edge", true)}Optional (only the generator)</span><span>${line("--out")}Depends on</span><span>${line("--in")}Used by</span></div>
<figure class="graph">
<div class="scroll">${map.svg}</div>
<div class="panel detail-empty"><p class="muted">Select a crate. Its panel shows what it is for, what it depends on, and what uses it.</p></div>
${map.panels}
</figure>
<details class="rawblock"><summary>The crate map as a table</summary><div class="scroll"><table><tr><th>Crate</th><th>Class</th><th>Depends on</th></tr>
${map.rows}
</table></div></details>

<h2 id="bus">The bus</h2>
<p class="lede">A capability reads documents by their content hash. Capabilities never call each other: they share data only as documents. A line joins a capability to each kind of document it reads. Select a capability to see what it reads, or a kind to see who reads it.</p>
<p class="stats">${capabilities.length} capabilities · ${bus.kinds} kinds of document · ${bus.reads} reads</p>
<div class="legend"><span>${swatch("kind")}Kept in the bucket</span><span>${swatch("kind", "hollow")}Not kept: derived, or kept on the device</span><span>${swatch("kind", "square")}An outside format</span><span>${line("--out")}Reads</span><span>${line("--in")}Read by</span></div>
<figure class="graph">
<div class="scroll">${bus.svg}</div>
<div class="panel detail-empty"><p class="muted">Select a capability or a kind of document.</p></div>
${bus.panels}
</figure>
<details class="rawblock"><summary>The bus as a table</summary><div class="scroll"><table><tr><th>Capability</th><th>Reads</th></tr>
${bus.rows}
</table></div></details>

<h2 id="traces">Two traces</h2>
<p class="lede">The maps above show what exists. These two show what happens: one question a page asks, and one change a person makes. Step through them with the arrows, or play them. Each step names the code that does it.</p>
${TRACES.map(traceSection).join("\n")}

<h2 id="bucket">Where things are kept</h2>
<p class="lede">The agency's record is one bucket in object storage. A file in it is written once and never changed. A kind is kept as a log when it is a collection of records that people change one at a time, such as stops, alerts, or notes. It is kept whole, in a pointer history, when it is replaced whole and rarely, such as the network or a build. The map shows where each kind lives, drawn from the kind table. The stories below it follow one of each into the bucket, and then a network a planner authors, which uses both.</p>
${bucketMap()}
${BUCKET_STORIES.map(bucketStory).join("\n")}

<h2 id="inputs">What each UI reads, and what it does through an outage</h2>
<p class="lede">Each UI declares the documents and feeds it reads from outside the page. One poll reads each of them while the page is visible, and stops a read at its time limit. A read answers a document, none kept, or a failure since a time. Each row below is drawn from the UI's declarations, beside the rows of its failure table that say what the page still shows while the input fails.</p>
${inputsSection()}

<h2 id="uis">What each UI links</h2>
<p class="lede">Each generated UI is its own product. Its page runs one module, linked from the capabilities it needs.</p>
${uiTable()}`,
    { wide: true, script: GRAPH_SCRIPT + TRACE_SCRIPT + BUCKET_SCRIPT },
  );
}

mkdirSync(out, { recursive: true });
const write = (name, html) => writeFileSync(join(out, name), html);
write("index.html", indexPage());
write("architecture.html", architecturePage());
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
