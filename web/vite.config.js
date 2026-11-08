import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import path from "node:path";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

// Pages: the picker (/) and every generated UI —
// one per `src/uis/<id>/` directory, served from `<id>.html` at the
// web root (asset fetches are relative, so every page lives at the
// root). Adding a UI is adding its directory and its html; a
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

// The service worker (web/service-worker.js — D-63, D-65): written to
// <outDir>/sw.js after the build with the build's identity and the
// shell it precaches above the template — the pages of every UI whose
// ui.json says `"offline": true`, every hashed asset, the capability
// manifest, and those UIs' linked assemblies. BUILD hashes the shell's
// bytes, so a build that changes any of them is a byte-different
// worker the browser's update check installs (and leaves waiting).
const offline = uis.filter((id) => JSON.parse(fs.readFileSync(path.join(root, "src/uis", id, "ui.json"), "utf8")).offline === true);
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
      fs.writeFileSync(path.join(outDir, "sw.js"), `const BUILD = ${JSON.stringify(build)};\nconst SHELL = ${JSON.stringify(shell)};\n\n${template}`);
    },
  };
}

export default defineConfig({
  plugins: [svelte(), serviceWorker()],
  build: { target: "esnext", rollupOptions: { input } },
  // The bus worker (src/bus.worker.ts) is a module worker.
  worker: { format: "es" },
  server: {
    // `npm run dev` proxies the sync server's routes (server/main.go):
    // the published documents, the manifest, edit sync and the store,
    // the geocoder, and /rt — the dev feed relay the dev
    // live-feeds.json names, for feeds that send no CORS header (D-67;
    // `just dev` runs the server with -rt-proxy).
    proxy: Object.fromEntries(
      ["/rt", "/manifest", "/network", "/streets", "/walks", "/edits", "/pointers", "/blobs", "/permissions", "/plans", "/feed", "/geocode"].map(
        (route) => [route, "http://127.0.0.1:8080"],
      ),
    ),
  },
});
