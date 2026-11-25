// The offline pages' service worker (D-63, D-65, D-189). The lifecycle is
// in docs/architecture/updates-offline.md. `npm run build` writes it to
// dist/sw.js with BUILD, SHELL, and OFFLINE above this text
// (vite.config.js), so every build is a byte-different worker.
//
// The worker's scope is the deployment's root, so it also controls the
// picker. That page's capability manifest, asked with `?ui=<id>`, must
// come from the network, because the shell's copy names this build's
// modules and goes stale at the next build. Hash-named files are safe to
// share, because their bytes never change.
//
// Paths are read relative to the scope, so a deployment under a prefix (a
// static host's /ride/, D-131) matches as one at the root.

/* global BUILD, SHELL, OFFLINE */

const SHELL_CACHE = `shell-${BUILD}`;
const FOREVER = "content-addressed";
const LAST = "last-published";
const BUILDS = "builds";

const SCOPE = new URL(self.registration.scope).pathname;
const SHELL_URLS = new Set(SHELL.map((p) => new URL(p, self.registration.scope).pathname));
const HASHED = [/^assets\/[^/]+-[A-Za-z0-9_-]{8}\.[a-z0-9]+$/, /^caps\/[^/]+-[0-9a-f]{64}\.wasm$/, /^blobs\/sha256:[0-9a-f]{64}(\.gz)?$/];
const PUBLISHED = ["manifest", "network", "network/document", "network/boundary", "streets", "walks", "live-feeds.json"];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(SHELL_CACHE).then((cache) => cache.addAll(SHELL)));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const meta = await caches.open(BUILDS);
      const seen = (await (await meta.match("order"))?.json()) ?? [];
      const order = [...seen.filter((b) => b !== BUILD), BUILD].slice(-2);
      await meta.put("order", new Response(JSON.stringify(order)));
      for (const key of await caches.keys()) {
        if (key.startsWith("shell-") && !order.some((b) => key === `shell-${b}`)) await caches.delete(key);
      }
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("message", (event) => {
  if (event.data?.type === "apply") self.skipWaiting();
  if (event.data?.type === "build") event.ports[0]?.postMessage(BUILD);
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  const path = url.pathname;
  if (!path.startsWith(SCOPE)) return;
  const rel = path.slice(SCOPE.length);
  const ui = url.searchParams.get("ui");
  if (rel === "caps/manifest.json" && ui && !OFFLINE.includes(ui)) return;
  if (request.mode === "navigate" || SHELL_URLS.has(path) || rel.startsWith("assets/")) {
    const page = request.mode === "navigate" && path.endsWith("/") ? `${path}index.html` : path;
    if (SHELL_URLS.has(page) || rel.startsWith("assets/")) event.respondWith(fromShell(request, page));
    return;
  }
  if (HASHED.some((re) => re.test(rel))) {
    event.respondWith(forever(request));
    return;
  }
  if (PUBLISHED.includes(rel) || rel.startsWith("basemap-assets/")) event.respondWith(networkElseLast(request));
});

// A page's query string is the page's state, not another file.
async function fromShell(request, path) {
  const cache = await caches.open(SHELL_CACHE);
  return (await cache.match(path)) ?? (await caches.match(path)) ?? fetch(request);
}

async function forever(request) {
  const cache = await caches.open(FOREVER);
  const hit = await cache.match(request);
  if (hit) return hit;
  const resp = await fetch(request);
  if (resp.ok) await cache.put(request, resp.clone());
  return resp;
}

// The `X-Served-From: last-copy` marker lets a page tell that the server
// did not answer. A display applies a waiting build only while it does.
async function networkElseLast(request) {
  const cache = await caches.open(LAST);
  try {
    const resp = await fetch(request);
    if (resp.ok) await cache.put(request, resp.clone());
    else if (resp.status >= 500) return lastCopy(await cache.match(request)) ?? resp;
    return resp;
  } catch (e) {
    const hit = lastCopy(await cache.match(request));
    if (hit) return hit;
    throw e;
  }
}

function lastCopy(hit) {
  if (!hit) return undefined;
  const headers = new Headers(hit.headers);
  headers.set("X-Served-From", "last-copy");
  return new Response(hit.body, { status: hit.status, statusText: hit.statusText, headers });
}
