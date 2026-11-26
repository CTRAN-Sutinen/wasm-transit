// tools/kind-trace.mjs: every kind in the kind table traces with nothing
// missing, and a printed handler, op, or route that does not exist fails.

import { test } from "node:test";
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { context, findKind, muxRoutes, trace } from "./kind-trace.mjs";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const ctx = context(ROOT);

test("every kind in the kind table traces to code that exists", () => {
  assert.ok(ctx.table.length > 10);
  for (const k of ctx.table) {
    const t = trace(ctx, k);
    assert.deepEqual(t.missing, [], `${k.store_kind}: ${t.missing.join("; ")}`);
    assert.ok(t.routes.length > 0 || t.no_route, `${k.store_kind} names its routes or says it has none`);
    for (const r of [...t.routes, ...(t.log?.routes ?? []), ...t.read.routes]) assert.match(r.at, /^server\/.+\.go:\d+$/);
  }
});

test("a kind's write route names its handler, and its check names the library", () => {
  const t = trace(ctx, findKind(ctx.table, "stops"));
  assert.ok(t.routes.some((r) => r.route === "POST /stops" && r.handler));
  assert.equal(t.check.op, "validate");
  assert.match(t.check.arm, /^crates\/cap-stops\/src\/.+\.rs:\d+$/);
  assert.ok(t.check.library.some((l) => l.startsWith("crates/transit-stops/")));
  assert.equal(t.log.name, "stops");
  assert.ok(t.store.includes("logs/stops/entries/<seq>.json"));
});

test("a kind with no route says so", () => {
  const t = trace(ctx, findKind(ctx.table, "boundary"));
  assert.deepEqual(t.routes, []);
  assert.match(t.no_route, /no HTTP route/);
});

const stops = findKind(ctx.table, "stops");

test("a handler that does not exist fails", () => {
  const gone = ctx.mux.find((r) => r.field?.endsWith("Post.Path")).handler;
  const broken = { ...ctx, funcs: { ...ctx.funcs, [gone]: undefined } };
  assert.ok(trace(broken, stops).missing.some((m) => /POST \/stops: no Go func/.test(m)));
});

test("an op that does not exist fails", () => {
  assert.ok(trace(ctx, { ...stops, validate: "no_such_op" }).missing.some((m) => /cap-stops has no op no_such_op/.test(m)));
  assert.ok(trace(ctx, { ...stops, module: "nothing" }).missing.some((m) => /no capability nothing/.test(m)));
});

test("a route that no mux pattern serves fails", () => {
  assert.ok(trace(ctx, { ...stops, route: "POST /nowhere" }).missing.some((m) => /POST \/nowhere: no mux pattern/.test(m)));
});

test("the mux reader finds a literal pattern and a kind table field", () => {
  const routes = muxRoutes(`\tmux.HandleFunc("/edits", s.protectWrite(func(*http.Request) string { return "edits" }, s.handleEdits))
\t\t\t\tmux.HandleFunc(k.Post.Path, s.protectWrite(s.keptRouteRule, s.handleKeptRoute))`);
  assert.deepEqual(routes, [{ path: "/edits", handler: "handleEdits" }, { field: "k.Post.Path", handler: "handleKeptRoute" }]);
});
