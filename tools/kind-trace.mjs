// The path of one kind through the code, from the kind table
// (`caps-run kind <tag>`). The server registers a kind's routes from the
// table, so a grep of a route finds no handler. This reads the table, the
// server's mux (`routes()` in server/main.go), the registry's op lists,
// and each capability's dispatch, and names the code at each step.
//
// A step it cannot find goes in `missing`. tools/kind-trace.test.mjs
// fails on any, for every kind in the table.

import { readdirSync, readFileSync, existsSync } from "node:fs";
import { join, relative } from "node:path";

const METHOD_PATH = /\b(GET|POST|PUT|PATCH|DELETE) (\/[^\s,;]*)/g;

function walk(dir, keep) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(path, keep));
    else if (keep(path)) out.push(path);
  }
  return out;
}

const camel = (op) => op.replace(/(^|_)([a-z0-9])/g, (_, __, c) => c.toUpperCase());

// Every `mux.HandleFunc(PATTERN, ... HANDLER))` in the server's routes().
// A pattern that is not a literal is a kind table field, such as
// `k.Post.Path`.
export function muxRoutes(mainGo) {
  const routes = [];
  for (const line of mainGo.split("\n")) {
    const m = /mux\.HandleFunc\(\s*("([^"]*)"|[\w.]+)\s*,(.*)$/.exec(line);
    if (!m) continue;
    const handler = /([A-Za-z_]\w*)\)*\s*$/.exec(m[3].replace(/\)+\s*$/, ""))?.[1];
    routes.push(m[2] !== undefined ? { path: m[2], handler } : { field: m[1], handler });
  }
  return routes;
}

// file:line of each Go func and method by name, outside the tests.
export function goFuncs(root) {
  const funcs = {};
  for (const file of walk(join(root, "server"), (p) => p.endsWith(".go") && !p.endsWith("_test.go"))) {
    readFileSync(file, "utf8").split("\n").forEach((line, i) => {
      const m = /^func (?:\([^)]*\) )?(\w+)[[(]/.exec(line);
      if (m) (funcs[m[1]] ??= []).push(`${relative(root, file)}:${i + 1}`);
    });
  }
  return funcs;
}

// The op's arm in the capability's dispatch, and the library functions it
// calls: transit_* paths, and methods that a transit-* dependency defines
// as `pub fn`. A local function the arm calls is read one level down.
const COMMON = new Set("clone map map_err to_string iter collect into as_ref unwrap_or ok get len is_empty push insert contains into_iter cloned to_owned unwrap expect and_then filter as_str as_deref unwrap_or_default new from default read".split(" "));

export function opArm(root, capability, op) {
  const dir = join(root, "crates", `cap-${capability}`);
  if (!existsSync(dir)) return null;
  const files = walk(join(dir, "src"), (p) => p.endsWith(".rs"));
  const head = new RegExp(`^(\\s*)Request::${camel(op)}\\b\\s*(\\{|\\(|=>)`);
  for (const file of files) {
    const lines = readFileSync(file, "utf8").split("\n");
    const at = lines.findIndex((l) => head.test(l));
    if (at < 0) continue;
    const indent = head.exec(lines[at])[1];
    let end = at + 1;
    while (end < lines.length && !(lines[end].startsWith(indent) && !lines[end].startsWith(indent + " ") && /\S/.test(lines[end]))) end++;
    const body = lines.slice(at, end).join("\n");
    return { arm: `${relative(root, file)}:${at + 1}`, library: libraryCalls(root, dir, files, body) };
  }
  return null;
}

function libraryCalls(root, capDir, capFiles, body) {
  const deps = [...readFileSync(join(capDir, "Cargo.toml"), "utf8").matchAll(/^(transit-[\w-]+)\s*=/gm)].map((m) => m[1]);
  const defs = new Map();
  for (const dep of deps) {
    const src = join(root, "crates", dep, "src");
    if (!existsSync(src)) continue;
    for (const file of walk(src, (p) => p.endsWith(".rs"))) {
      readFileSync(file, "utf8").split("\n").forEach((line, i) => {
        const m = /^\s*pub(?:\([^)]*\))? (?:const )?fn (\w+)/.exec(line);
        if (m && !defs.has(`${dep}:${m[1]}`)) defs.set(`${dep}:${m[1]}`, `${relative(root, file)}:${i + 1}`);
      });
    }
  }
  const local = new Map();
  for (const file of capFiles) {
    for (const m of readFileSync(file, "utf8").matchAll(/^fn (\w+)\([\s\S]*?\n\}/gm)) local.set(m[1], m[0]);
  }
  let text = body;
  for (const m of body.matchAll(/(?<![\w.:])(\w+)\(/g)) if (local.has(m[1])) text += "\n" + local.get(m[1]);
  const found = new Set();
  for (const m of text.matchAll(/\b(transit_\w+)::(?:[A-Z]\w*::)*([a-z_]\w*)\s*\(/g)) {
    const at = defs.get(`${m[1].replace(/_/g, "-")}:${m[2]}`);
    if (at) found.add(`${at} ${m[1]}::${m[2]}`);
  }
  // A method resolves in the capability's own library first.
  const home = capDir.replace(/.*cap-/, "transit-");
  for (const m of text.matchAll(/\.([a-z_]\w*)\(/g)) {
    if (COMMON.has(m[1])) continue;
    for (const dep of defs.has(`${home}:${m[1]}`) ? [home] : deps) {
      const at = defs.get(`${dep}:${m[1]}`);
      if (at) found.add(`${at} ${dep.replace(/-/g, "_")}::${m[1]}`);
    }
  }
  return [...found];
}

export function context(root) {
  const registry = (id) => {
    const file = join(root, "registry", "capabilities", `${id}.json`);
    return existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : null;
  };
  const kernel = JSON.parse(readFileSync(join(root, "registry", "registry.json"), "utf8")).kernel_ops ?? [];
  const capabilities = readdirSync(join(root, "registry", "capabilities")).map((f) => registry(f.replace(/\.json$/, "")));
  return {
    root,
    table: JSON.parse(readFileSync(join(root, "web", "pkg", "kinds.json"), "utf8")).kinds,
    mux: muxRoutes(readFileSync(join(root, "server", "main.go"), "utf8")),
    funcs: goFuncs(root),
    registry,
    kernel,
    capabilities,
  };
}

export function findKind(table, name) {
  return table.find((k) => k.tag === name && name !== "") ?? table.find((k) => k.store_kind === name);
}

export function trace(ctx, k) {
  const missing = [];
  const handlerAt = (handler, what) => {
    const at = ctx.funcs[handler]?.[0];
    if (!at) missing.push(`${what}: no Go func ${handler} in server/`);
    return at ?? null;
  };
  const route = (method, path) => {
    let entry = ctx.mux.find((r) => r.path === path);
    if (!entry && k.post && path === k.post.path) entry = ctx.mux.find((r) => r.field?.endsWith("Post.Path"));
    if (!entry && k.work_order && path === k.work_order.path) entry = ctx.mux.find((r) => r.field?.endsWith("WorkOrder.Path"));
    if (!entry) {
      entry = ctx.mux
        .filter((r) => r.path?.endsWith("/") && r.path !== "/" && path.startsWith(r.path))
        .sort((a, b) => b.path.length - a.path.length)[0];
    }
    const name = `${method} ${path}`;
    if (!entry) {
      missing.push(`${name}: no mux pattern in server/main.go routes() serves it`);
      return { route: name, handler: null };
    }
    return { route: name, pattern: entry.path ?? entry.field, handler: entry.handler, at: handlerAt(entry.handler, name) };
  };
  const op = (capability, name, what) => {
    const record = ctx.registry(capability);
    if (!record) {
      missing.push(`${what}: no capability ${capability} in the registry`);
      return { capability, op: name };
    }
    if (!record.ops.includes(name)) missing.push(`${what}: cap-${capability} has no op ${name}`);
    const arm = opArm(ctx.root, capability, name);
    if (!arm) missing.push(`${what}: no Request::${camel(name)} arm in crates/cap-${capability}/src`);
    return { capability, op: name, ...arm };
  };

  const out = { kind: k.store_kind, tag: k.tag, versions: k.versions.map((v) => v.version), write: k.write };
  const written = [...k.route.matchAll(METHOD_PATH)].map((m) => route(m[1], m[2]));
  if (k.write === "pointer") for (const p of k.pointers) written.push(route("POST", `/pointers/${p}`));
  if (k.work_order) written.push(route(k.work_order.method, k.work_order.path));
  const seen = new Set();
  out.routes = written.filter((r) => !seen.has(r.route) && seen.add(r.route));
  if (out.routes.length === 0) out.no_route = `no HTTP route writes it: ${k.route}`;

  if (k.post) out.post = { path: k.post.path, answers: k.post.answer, ...op(k.module, k.post.op, `post ${k.post.path}`) };
  if (k.work_order) {
    out.work_order = {
      call: op(k.module, k.work_order.call, "work_order.call"),
      record: op(k.module, k.work_order.record, "work_order.record"),
    };
  }
  if (k.log) {
    const intent = k.intent !== false;
    out.log = {
      name: k.log,
      receive: k.receive ? op(k.receive, "receive", `the ${k.log} log's receive`) : null,
      records: k.records ?? [],
      intent,
      routes: [
        route("GET", `/logs/${k.log}`),
        ...(intent ? [route("POST", `/logs/${k.log}`)] : []),
        route("POST", `/logs/${k.log}/revert`),
        route("POST", `/logs/${k.log}/close-gap`),
      ],
      fold: op("log", "fold", "the log fold"),
    };
    if (k.receive && k.module && k.records?.length) out.log.document = op(k.module, "log_document", "the log's document");
  }
  if (k.load) {
    const kernel = ctx.kernel.find((x) => x.code === k.load.code);
    if (!kernel) missing.push(`load: no kernel op ${k.load.code} in the registry`);
    const id = kernel?.kind ?? k.load.op.replace(/^load_/, "");
    out.load = {
      op: k.load.op,
      code: k.load.code,
      read_by: ctx.capabilities.filter((c) => (c.documents ?? []).includes(id)).map((c) => c.id),
    };
  }
  if (k.module && k.validate) out.check = op(k.module, k.validate, "check");
  if (k.pick) out.pick = { seq: k.pick.seq, export: op(k.module, k.pick.export, "pick.export") };
  if (k.feed_events) out.feed_events = op(k.module, k.feed_events, "feed_events");
  out.merge = k.merge;
  out.read = {
    rights: k.read,
    routes: [
      route("GET", "/manifest"),
      route("GET", "/blobs/sha256:<hex>"),
      ...k.pointers.filter((p) => !p.endsWith("*")).map((p) => route("GET", `/pointers/${p}`)),
    ],
  };
  out.store = [
    "blobs/<hex>",
    ...k.pointers.map((p) => `pointers/${p.endsWith("*") ? p.slice(0, -1) + "<id>" : p}/<seq>.json`),
    ...(k.log ? [`logs/${k.log}/entries/<seq>.json`, `logs/${k.log}/checkpoints/<seq>.json`] : []),
    ...(k.tag ? k.versions.map((v) => `schemas/${k.tag}.v${v.version}/<hex>.json`) : []),
  ];
  out.missing = missing;
  return out;
}
