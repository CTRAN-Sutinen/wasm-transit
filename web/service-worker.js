/// The rider pages' service worker (D-63, D-65) — ONE lifecycle for
/// updates and offline. `npm run build` writes it to dist/sw.js with
/// BUILD (the build's identity: a hash over the shell's files) and
/// SHELL (the files it precaches) above this text (vite.config.ts,
/// "the service worker"), so every build is a byte-different worker
/// and the browser's update check finds it.
///
/// - Install precaches the shell — the rider pages, their scripts and
///   styles, the capability manifest, and each rider page's linked
///   assembly — and WAITS: a new build never takes over an open page.
///   The page applies it (src/uis/kit.ts, "the app's build"): a
///   rider's at the next cold start or navigation, or when they accept
///   "a new version is ready"; a display at its next poll while the
///   server answers. `apply` is the page's message.
/// - Activate takes the open pages and keeps this build's shell and the
///   one before it (a page another tab moved past still loads the
///   files of the build it runs); older shells are dropped.
/// - Fetch answers: the shell from its precache; content-addressed
///   files (Vite's hashed assets, the hash-named modules, every
///   published document at blobs/<hash>[.gz]) from a cache filled on
///   first use, forever; the deployment's published documents and
///   config from the network, else the last copy — a page boots
///   offline on the last manifest it had, and so on the network it
///   pins (D-65). Paths are
///   read relative to the worker's scope, so a deployment under a
///   prefix (a static host's /rider/, D-131) matches as one at the root.
///   Never cached: live feeds and the dev relay (a prediction is never
///   replayed as live), the edit document (offline, the boards are the
///   schedule and say so), writes, and the geocoder.

/* global BUILD, SHELL */

const SHELL_CACHE = `shell-${BUILD}`;
const FOREVER = "content-addressed";
const LAST = "last-published";
/// The builds activated, oldest first (one JSON response).
const BUILDS = "builds";

const SCOPE = new URL(self.registration.scope).pathname;
const SHELL_URLS = new Set(SHELL.map((p) => new URL(p, self.registration.scope).pathname));
/// Scope-relative paths (no leading slash).
const HASHED = [/^assets\/[^/]+-[A-Za-z0-9_-]{8}\.[a-z0-9]+$/, /^caps\/[^/]+-[0-9a-f]{64}\.wasm$/, /^blobs\/sha256:[0-9a-f]{64}(\.gz)?$/];
const PUBLISHED = ["manifest", "network/document", "network/boundary", "streets", "walks", "live-feeds.json"];

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

/// The shell's copy (a page's query string is the page's state, not
/// another file).
async function fromShell(request, path) {
  const cache = await caches.open(SHELL_CACHE);
  return (await cache.match(path)) ?? (await caches.match(path)) ?? fetch(request);
}

/// Content-addressed: the bytes never change, so the first copy is
/// kept forever.
async function forever(request) {
  const cache = await caches.open(FOREVER);
  const hit = await cache.match(request);
  if (hit) return hit;
  const resp = await fetch(request);
  if (resp.ok) await cache.put(request, resp.clone());
  return resp;
}

/// The deployment's current copy, kept as the last one; when the
/// server does not answer, the last copy — marked `X-Served-From:
/// last-copy`, so a page can tell the server did not answer (a display
/// applies a waiting build only while it does).
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
