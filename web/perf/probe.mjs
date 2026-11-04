// The perf probe: what a perf spec measures around each step, through
// the Chrome DevTools Protocol — the browser's own counters, a sampled
// CPU profile, the long animation frames, and each capability module's
// linear memory (which the JS heap counters do not see). Test-side
// only: the page under test is the shipped page, observed.

import fs from "node:fs";
import path from "node:path";

/// Installed before any page script: observes long animation frames
/// (with their script attribution) and long tasks. The capability
/// modules run in the page's bus worker, so their memory is read
/// through the engine's inspection (`window.__bus`) and their CPU from
/// the worker's own profiler.
function pageProbe() {
  const perf = (window.__perf = { loaf: [], longtasks: [] });
  try {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) {
        perf.loaf.push({
          duration: e.duration,
          blocking: e.blockingDuration,
          scripts: e.scripts.map((s) => ({ fn: s.sourceFunctionName, src: s.sourceURL, invoker: s.invoker, duration: s.duration })),
        });
      }
    }).observe({ type: "long-animation-frame", buffered: true });
  } catch {}
  try {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) perf.longtasks.push(e.duration);
    }).observe({ type: "longtask", buffered: true });
  } catch {}
}

const MB = 1024 * 1024;
const round = (x, d = 1) => Math.round(x * 10 ** d) / 10 ** d;

export class Probe {
  constructor(page, outDir) {
    this.page = page;
    this.outDir = outDir;
    this.steps = [];
    this.samples = [];
    fs.mkdirSync(path.join(outDir, "profiles"), { recursive: true });
  }

  async start() {
    await this.page.addInitScript(pageProbe);
    this.cdp = await this.page.context().newCDPSession(this.page);
    await this.cdp.send("Performance.enable", { timeDomain: "threadTicks" });
    await this.cdp.send("Profiler.enable");
    await this.cdp.send("Profiler.setSamplingInterval", { interval: 200 });
  }

  /// The bus worker's own DevTools session (once the page spawned it):
  /// the capability modules run there, so their CPU is profiled there.
  /// Attached through the browser session, unflattened — Playwright's
  /// CDP sessions reach pages, not workers.
  async worker() {
    if (this.bus) return this.bus;
    const browser = await this.page.context().browser().newBrowserCDPSession();
    const { targetInfos } = await browser.send("Target.getTargets");
    const target = targetInfos.find((t) => t.type === "worker" && /bus\.worker/.test(t.url));
    if (!target) return null;
    const { sessionId } = await browser.send("Target.attachToTarget", { targetId: target.targetId, flatten: false });
    let next = 1;
    const waiting = new Map();
    browser.on("Target.receivedMessageFromTarget", (e) => {
      if (e.sessionId !== sessionId) return;
      const msg = JSON.parse(e.message);
      const w = waiting.get(msg.id);
      if (!w) return;
      waiting.delete(msg.id);
      if (msg.error) w.reject(new Error(msg.error.message));
      else w.resolve(msg.result);
    });
    const send = (method, params = {}) =>
      new Promise((resolve, reject) => {
        const id = next++;
        waiting.set(id, { resolve, reject });
        browser.send("Target.sendMessageToTarget", { sessionId, message: JSON.stringify({ id, method, params }) }).catch(reject);
      });
    await send("Profiler.enable");
    await send("Profiler.setSamplingInterval", { interval: 200 });
    this.bus = { send };
    return this.bus;
  }

  async counters() {
    const { metrics } = await this.cdp.send("Performance.getMetrics");
    const m = Object.fromEntries(metrics.map((x) => [x.name, x.value]));
    return {
      taskMs: m.TaskDuration * 1000,
      scriptMs: m.ScriptDuration * 1000,
      layoutMs: m.LayoutDuration * 1000,
      styleMs: m.RecalcStyleDuration * 1000,
    };
  }

  /// Memory after a full collection: the V8 heap, the array buffers
  /// outside it, each capability module's linear memory, the DOM.
  async memory() {
    await this.cdp.send("HeapProfiler.collectGarbage");
    const heap = await this.cdp.send("Runtime.getHeapUsage");
    const { metrics } = await this.cdp.send("Performance.getMetrics");
    const m = Object.fromEntries(metrics.map((x) => [x.name, x.value]));
    const wasm = await this.page.evaluate(async () => {
      if (!window.__bus) return {};
      const caps = await window.__bus.caps();
      return Object.fromEntries(
        await Promise.all(
          caps.map(async (cap) => {
            const [{ memory, resets }, { held }] = await Promise.all([window.__bus.memory(cap), window.__bus.held(cap)]);
            return [cap, { memory, resets, held: held.length }];
          }),
        ),
      );
    });
    const wasmTotal = Object.values(wasm).reduce((a, b) => a + b.memory, 0);
    const bus = await this.worker().catch(() => null);
    const workerHeap = bus ? await bus.send("Runtime.getHeapUsage").catch(() => null) : null;
    return {
      jsHeapMB: round(heap.usedSize / MB),
      workerHeapMB: workerHeap ? round(workerHeap.usedSize / MB) : null,
      arrayBuffersMB: round((heap.backingStorageSize ?? 0) / MB),
      wasmMB: round(wasmTotal / MB),
      wasmByCapMB: Object.fromEntries(Object.entries(wasm).map(([k, v]) => [k, round(v.memory / MB)])),
      /// Per module: documents held (every registry) and instance resets.
      heldByCap: Object.fromEntries(Object.entries(wasm).map(([k, v]) => [k, v.held])),
      resetsByCap: Object.fromEntries(Object.entries(wasm).map(([k, v]) => [k, v.resets])),
      domNodes: m.Nodes,
      listeners: m.JSEventListeners,
    };
  }

  async sample(label) {
    const s = { label, ...(await this.memory()) };
    this.samples.push(s);
    return s;
  }

  /// One measured step: wall time, main-thread time, long frames, a
  /// CPU profile (saved, and summarized by self time), memory after.
  async step(name, fn, { profile = true } = {}) {
    await this.page.evaluate(() => {
      window.__perf.loaf.length = 0;
      window.__perf.longtasks.length = 0;
    });
    const before = await this.counters();
    const bus = profile ? await this.worker() : null;
    if (profile) await this.cdp.send("Profiler.start");
    if (bus) await bus.send("Profiler.start");
    const t0 = performance.now();
    await fn();
    const wallMs = performance.now() - t0;
    let top = null;
    if (profile) {
      const { profile: p } = await this.cdp.send("Profiler.stop");
      const file = `${String(this.steps.length).padStart(2, "0")}-${name.replace(/[^a-z0-9]+/gi, "-")}.cpuprofile`;
      fs.writeFileSync(path.join(this.outDir, "profiles", file), JSON.stringify(p));
      top = summarize(p);
      top.file = `profiles/${file}`;
      if (bus) {
        const { profile: w } = await bus.send("Profiler.stop");
        const wfile = file.replace(/\.cpuprofile$/, "-worker.cpuprofile");
        fs.writeFileSync(path.join(this.outDir, "profiles", wfile), JSON.stringify(w));
        top.worker = { ...summarize(w), file: `profiles/${wfile}` };
      }
    }
    const after = await this.counters();
    const frames = await this.page.evaluate(() => ({ loaf: window.__perf.loaf.slice(), longtasks: window.__perf.longtasks.slice() }));
    const mem = await this.memory();
    const step = {
      name,
      wallMs: round(wallMs),
      mainThreadMs: round(after.taskMs - before.taskMs),
      scriptMs: round(after.scriptMs - before.scriptMs),
      layoutMs: round(after.layoutMs - before.layoutMs),
      styleMs: round(after.styleMs - before.styleMs),
      longTasks: frames.longtasks.length,
      longestTaskMs: round(Math.max(0, ...frames.longtasks)),
      longFrames: frames.loaf.length,
      blockingMs: round(frames.loaf.reduce((a, f) => a + f.blocking, 0)),
      worstFrames: frames.loaf
        .sort((a, b) => b.duration - a.duration)
        .slice(0, 3)
        .map((f) => ({
          ms: round(f.duration),
          scripts: f.scripts.slice(0, 3).map((s) => `${s.invoker} → ${s.fn || "(anon)"} ${round(s.duration)}ms`),
        })),
      memory: mem,
      profile: top,
    };
    this.steps.push(step);
    return step;
  }

  write(extra = {}) {
    const report = { ...extra, steps: this.steps, samples: this.samples };
    fs.writeFileSync(path.join(this.outDir, "report.json"), JSON.stringify(report, null, 2));
    fs.writeFileSync(path.join(this.outDir, "summary.md"), markdown(report));
    return report;
  }
}

/// Self time per function and per bucket (a capability module's crate,
/// a JS bundle file, the engine's own work) from a sampled profile.
/// `(program)` — native main-thread work V8 cannot attribute (in
/// headless Chromium mostly the software-WebGL map) — is reported
/// apart from the attributed busy time.
export function summarize(profile) {
  const byId = new Map(profile.nodes.map((n) => [n.id, n]));
  const self = new Map();
  profile.samples.forEach((id, i) => self.set(id, (self.get(id) ?? 0) + (profile.timeDeltas[i + 1] ?? 0)));
  const fns = new Map();
  const buckets = new Map();
  let total = 0;
  let program = 0;
  for (const [id, us] of self) {
    const { functionName, url, lineNumber } = byId.get(id).callFrame;
    if (functionName === "(idle)") continue;
    if (functionName === "(program)") {
      program += us;
      continue;
    }
    total += us;
    const wasm = url.startsWith("wasm://");
    const name = wasm ? demangle(functionName) : functionName || "(anon)";
    const bucket = bucketOf(name, url);
    const key = `${name.length > 140 ? `${name.slice(0, 140)}…` : name} [${bucket}${url && !wasm ? `:${lineNumber + 1}` : ""}]`;
    fns.set(key, (fns.get(key) ?? 0) + us);
    buckets.set(bucket, (buckets.get(bucket) ?? 0) + us);
  }
  const ms = (us) => round(us / 1000);
  const sorted = (m, n) => [...m].sort((a, b) => b[1] - a[1]).slice(0, n).map(([k, v]) => ({ name: k, ms: ms(v) }));
  return { busyMs: ms(total), programMs: ms(program), buckets: sorted(buckets, 12), functions: sorted(fns, 20) };
}

/// A Rust v0 symbol (`_RNvNtCs…_4sha26sha25611compress256`) as its
/// path's identifiers (`sha2::sha256::compress256`) — enough to read
/// and bucket, not a full demangler (generic arguments flatten into
/// the path). Anything else passes through.
export function demangle(sym) {
  if (!sym.startsWith("_R")) return sym;
  const s = sym.slice(2);
  const idents = [];
  for (let i = 0; i < s.length; ) {
    // A crate root or an impl carries a disambiguator: s<base62>_.
    if ("CMXY".includes(s[i]) && s[i + 1] === "s") {
      i = s.indexOf("_", i) + 1 || s.length;
      continue;
    }
    const m = /^\d+/.exec(s.slice(i));
    if (!m) {
      i++;
      continue;
    }
    let j = i + m[0].length;
    if (s[j] === "_") j++;
    const n = Number(m[0]);
    const ident = s.slice(j, j + n);
    if (n > 0 && ident.length === n && /^[A-Za-z_][A-Za-z0-9_]*$/.test(ident)) {
      if (idents[idents.length - 1] !== ident) idents.push(ident);
      i = j + n;
    } else i = j;
  }
  return idents.length ? idents.join("::") : sym;
}

function bucketOf(fn, url) {
  if (!url) return fn.startsWith("(") ? fn : "native";
  if (url.startsWith("wasm://")) {
    // The name section names Rust paths: the crate is the bucket.
    const crate = /^([A-Za-z_][A-Za-z0-9_]*)::/.exec(fn)?.[1];
    return crate ? `wasm:${crate}` : `wasm:${fn}`;
  }
  const file = url.split("/").pop().split("?")[0];
  return file.replace(/-[A-Za-z0-9_-]{8}\.js$/, ".js");
}

function markdown(r) {
  const out = [`# Perf baseline — ${r.ui}`, "", `${r.when} · feed ${r.feed} · live ${r.live} · ${r.build}`, ""];
  out.push("## Steps", "", "| step | wall ms | main-thread ms | script ms | layout+style ms | long tasks (max ms) | worker busy ms | JS heap MB | wasm MB | DOM nodes |", "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
  for (const s of r.steps) {
    out.push(
      `| ${s.name} | ${s.wallMs} | ${s.mainThreadMs} | ${s.scriptMs} | ${round(s.layoutMs + s.styleMs)} | ${s.longTasks} (${s.longestTaskMs}) | ${s.profile?.worker?.busyMs ?? ""} | ${s.memory.jsHeapMB} | ${s.memory.wasmMB} | ${s.memory.domNodes} |`,
    );
  }
  out.push("", "## Where the CPU went (self time, per step)", "");
  for (const s of r.steps.filter((s) => s.profile)) {
    out.push(`### ${s.name} — ${s.profile.busyMs} ms attributed, ${s.profile.programMs} ms (program) (\`${s.profile.file}\`)`, "");
    out.push(s.profile.buckets.map((b) => `${b.name} ${b.ms}`).join(" · "), "");
    for (const f of s.profile.functions.slice(0, 8)) out.push(`- ${f.ms} ms ${f.name}`);
    out.push("");
    const w = s.profile.worker;
    if (w) {
      out.push(`#### the bus worker — ${w.busyMs} ms attributed (\`${w.file}\`)`, "");
      out.push(w.buckets.map((b) => `${b.name} ${b.ms}`).join(" · "), "");
      for (const f of w.functions.slice(0, 8)) out.push(`- ${f.ms} ms ${f.name}`);
      out.push("");
    }
  }
  if (r.samples.length) {
    const caps = [...new Set(r.samples.flatMap((s) => Object.keys(s.wasmByCapMB)))];
    out.push("## Memory over time (after GC; per module: linear MB (documents held, resets))", "", `| sample | JS heap MB | worker heap MB | array buffers MB | DOM nodes | listeners | ${caps.join(" | ")} |`, `|---|---:|---:|---:|---:|---:|${caps.map(() => "---:").join("|")}|`);
    for (const s of r.samples) {
      out.push(`| ${s.label} | ${s.jsHeapMB} | ${s.workerHeapMB ?? ""} | ${s.arrayBuffersMB} | ${s.domNodes} | ${s.listeners} | ${caps.map((c) => (s.wasmByCapMB[c] == null ? "" : `${s.wasmByCapMB[c]} (${s.heldByCap?.[c] ?? "?"} held${s.resetsByCap?.[c] ? `, ${s.resetsByCap[c]} resets` : ""})`)).join(" | ")} |`);
    }
    out.push("");
  }
  if (r.ticks?.length) {
    const q = (xs, p) => round([...xs].sort((a, b) => a - b)[Math.min(xs.length - 1, Math.floor(p * xs.length))]);
    const main = r.ticks.map((t) => t.mainThreadMs);
    out.push(
      "## Poll ticks (soak)",
      "",
      `${r.ticks.length} ticks · main-thread ms per tick p50 ${q(main, 0.5)} · p90 ${q(main, 0.9)} · max ${q(main, 1)} · ticks with a long task ${r.ticks.filter((t) => t.longTasks).length}`,
      "",
    );
  }
  return out.join("\n");
}
