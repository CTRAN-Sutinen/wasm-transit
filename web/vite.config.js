import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import path from "node:path";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

// Pages: the picker (/) and every generated UI —
// one per `src/uis/<id>/` directory, served from `<id>.html` at the
// bundle's root (asset fetches are relative, so every page lives at
// the root of wherever the bundle is served). Adding a UI is adding its directory and its html; a
// directory without its page fails the build here, not at runtime.
const root = path.dirname(fileURLToPath(import.meta.url));
const uis = fs
  .readdirSync(path.join(root, "src/uis"), { withFileTypes: true })
  .filter((d) => d.isDirectory())
  .map((d) => d.name);
for (const id of uis) {
  for (const need of [`${id}.html`, `src/uis/${id}/ui.json`]) {
    if (!fs.existsSync(path.join(root, need))) throw new Error(`generated UI "${id}" is missing ${need}`);
  }
}
const input = {
  main: "index.html",
  ...Object.fromEntries(uis.map((id) => [id, `${id}.html`])),
};

// ONE UI, deployable on its own (`UI=<id> vite build` → dist-ui/<id>/;
// `just export <id> …`, D-131): only that page, its scripts, and what
// it runs — the capability manifest, its linked assembly and, for the
// fallback a page takes when the assembly does not load (D-88), each
// of its capabilities alone; the deployment's live-feeds.json; the
// basemap's files only when the page draws a map (its code includes
// src/basemap.ts). index.html forwards to the page. Without UI the
// build is every page (the sync server's).
const only = process.env.UI || null;
if (only && !uis.includes(only)) throw new Error(`UI=${only}: no generated UI by that id (${uis.join(", ")})`);
function oneUi(id) {
  let outDir = "";
  let drawsMap = false;
  const pub = path.join(root, "public");
  const copy = (rel) => {
    const from = path.join(pub, rel);
    if (fs.existsSync(from)) fs.cpSync(from, path.join(outDir, rel), { recursive: true });
  };
  return {
    name: "one-ui",
    apply: "build",
    configResolved(config) {
      outDir = path.resolve(config.root, config.build.outDir);
    },
    generateBundle(_, bundle) {
      drawsMap = Object.values(bundle).some((c) => c.type === "chunk" && Object.keys(c.modules).some((m) => m.endsWith("/src/basemap.ts")));
    },
    closeBundle() {
      const caps = JSON.parse(fs.readFileSync(path.join(pub, "caps/manifest.json"), "utf8"));
      const linked = caps.assemblies?.find((a) => a.ui === id);
      copy("caps/manifest.json");
      if (linked) {
        copy(linked.url);
        for (const cap of caps.capabilities.filter((c) => linked.capabilities.includes(c.id))) copy(cap.url);
      }
      copy("live-feeds.json");
      if (drawsMap) {
        copy("basemap.pmtiles");
        copy("basemap-assets");
      }
      fs.writeFileSync(
        path.join(outDir, "index.html"),
        `<!doctype html>\n<meta charset="utf-8">\n<title>${id}</title>\n<script>location.replace(${JSON.stringify(`${id}.html`)} + location.search + location.hash);</script>\n<a href="${id}.html">${id}</a>\n`,
      );
    },
  };
}

// The service worker (web/service-worker.js — D-63, D-65): written to
// <outDir>/sw.js after the build with the build's identity and the
// shell it precaches above the template (and OFFLINE, those UIs' ids)
// — the pages of every UI whose
// ui.json says `"offline": true`, every hashed asset, the capability
// manifest, and those UIs' linked assemblies. BUILD hashes the shell's
// bytes, so a build that changes any of them is a byte-different
// worker the browser's update check installs (and leaves waiting).
const offline = uis.filter((id) => (!only || id === only) && JSON.parse(fs.readFileSync(path.join(root, "src/uis", id, "ui.json"), "utf8")).offline === true);
function serviceWorker() {
  let outDir = "";
  return {
    name: "service-worker",
    apply: "build",
    configResolved(config) {
      outDir = path.resolve(config.root, config.build.outDir);
    },
    closeBundle() {
      const assetsDir = path.join(outDir, "assets");
      const assets = fs.existsSync(assetsDir) ? fs.readdirSync(assetsDir).sort().map((f) => `assets/${f}`) : [];
      const capsManifest = path.join(outDir, "caps/manifest.json");
      const caps = fs.existsSync(capsManifest) ? JSON.parse(fs.readFileSync(capsManifest, "utf8")) : null;
      const shell = [
        ...offline.map((id) => `${id}.html`),
        ...assets,
        ...(caps ? ["caps/manifest.json"] : []),
        ...(caps?.assemblies ?? []).filter((a) => offline.includes(a.ui)).map((a) => a.url),
      ];
      const hash = createHash("sha256");
      for (const file of shell) hash.update(file).update(fs.readFileSync(path.join(outDir, file)));
      const build = hash.digest("hex").slice(0, 16);
      const template = fs.readFileSync(path.join(root, "service-worker.js"), "utf8");
      fs.writeFileSync(
        path.join(outDir, "sw.js"),
        `const BUILD = ${JSON.stringify(build)};\nconst SHELL = ${JSON.stringify(shell)};\nconst OFFLINE = ${JSON.stringify(offline)};\n\n${template}`,
      );
    },
  };
}

export default defineConfig({
  // Relative: the built pages reference their assets beside them, so
  // a deployment serves them from any path — a static host's prefix
  // (/ride/, D-131) as well as the sync server's root.
  base: "./",
  plugins: [svelte(), ...(only ? [oneUi(only)] : []), serviceWorker()],
  build: only
    ? { target: "esnext", outDir: `dist-ui/${only}`, rollupOptions: { input: { [only]: `${only}.html` } } }
    : { target: "esnext", rollupOptions: { input } },
  // One UI copies only what it runs from public/ (oneUi).
  publicDir: only ? false : "public",
  // The bus worker (src/bus.worker.ts) is a module worker.
  worker: { format: "es" },
  server: {
    // `npm run dev` proxies the sync server's routes (server/main.go):
    // the published documents, the manifest, edit sync and the store,
    // the geocoder, and /rt — the dev feed relay the dev
    // live-feeds.json names, for feeds that send no CORS header (D-67;
    // `just dev` runs the server with -rt-proxy). A key is a path prefix,
    // or a pattern when it begins with ^: the stops document's writes are
    // `/stops` exactly, never the stops page (`/stops.html`); the stop
    // requests' writes are `/stop-requests` and the work order system's
    // adapter `/stop-requests/work-order`; photos are `/attachments`.
    proxy: Object.fromEntries(
      ["/rt", "/manifest", "/network", "/streets", "/walks", "/edits", "/pointers", "/blobs", "/permissions", "/plans", "^/stops$", "^/stop-requests(/|$)", "/attachments", "/feed", "/geocode"].map(
        // TRANSIT_SERVER names another server (`just qa` runs its own).
        (route) => [route, process.env.TRANSIT_SERVER ?? "http://127.0.0.1:8080"],
      ),
    ),
  },
});
