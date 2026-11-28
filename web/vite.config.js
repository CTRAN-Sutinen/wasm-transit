import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import path from "node:path";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

// Each `src/uis/<id>/` directory is a generated UI, served from `<id>.html`
// at the bundle's root. A directory without its page fails the build here,
// not at runtime.
const root = path.dirname(fileURLToPath(import.meta.url));
const uis = fs
  .readdirSync(path.join(root, "src/uis"), { withFileTypes: true })
  // The justfile's UI gate also skips hidden directories, which an editor
  // or a tool creates.
  .filter((d) => d.isDirectory() && !d.name.startsWith("."))
  .map((d) => d.name);
for (const id of uis) {
  for (const need of [`${id}.html`, `src/uis/${id}/ui.json`]) {
    if (!fs.existsSync(path.join(root, need)))
      throw new Error(`generated UI "${id}" is missing ${need}`);
  }
}
const input = {
  main: "index.html",
  ...Object.fromEntries(uis.map((id) => [id, `${id}.html`])),
};

// `UI=<id> vite build` writes dist-ui/<id>/ for `just export <id> …`.
// It holds only that page and what it runs: the capability
// manifest, its linked assembly, each of its capabilities alone (the
// fallback when the assembly does not load, D-88), live-feeds.json, the
// schemas, and the basemap files only when the page imports
// src/basemap.ts. Without UI the build is every page, as the sync server
// serves it.
const only = process.env.UI || null;
if (only && !uis.includes(only))
  throw new Error(`UI=${only}: no generated UI by that id (${uis.join(", ")})`);
function oneUi(id) {
  let outDir = "";
  let drawsMap = false;
  const pub = path.join(root, "public");
  const copy = (rel) => {
    const from = path.join(pub, rel);
    if (fs.existsSync(from))
      fs.cpSync(from, path.join(outDir, rel), { recursive: true });
  };
  return {
    name: "one-ui",
    apply: "build",
    configResolved(config) {
      outDir = path.resolve(config.root, config.build.outDir);
    },
    generateBundle(_, bundle) {
      drawsMap = Object.values(bundle).some(
        (c) =>
          c.type === "chunk" &&
          Object.keys(c.modules).some((m) => m.endsWith("/src/basemap.ts")),
      );
    },
    closeBundle() {
      const caps = JSON.parse(
        fs.readFileSync(path.join(pub, "caps/manifest.json"), "utf8"),
      );
      const linked = caps.assemblies?.find((a) => a.ui === id);
      copy("caps/manifest.json");
      if (linked) {
        copy(linked.url);
        for (const cap of caps.capabilities.filter((c) =>
          linked.capabilities.includes(c.id),
        ))
          copy(cap.url);
      }
      copy("live-feeds.json");
      copy("schemas");
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

// Writes web/service-worker.js to <outDir>/sw.js. The shell is
// the pages of every UI whose ui.json says `"offline": true`, every hashed
// asset, the capability manifest, and those UIs' linked assemblies. BUILD
// hashes the shell's bytes, so a build that changes any of them is a
// byte-different worker that the browser's update check installs.
// A UI with `"precache_assembly": false` (the staff pages, D-223) stays
// out of the shared shell, so a rider's install never downloads a
// planner's modules. A one-UI build (`only`) precaches its own assembly.
const uiJson = (id) =>
  JSON.parse(
    fs.readFileSync(path.join(root, "src/uis", id, "ui.json"), "utf8"),
  );
const offline = uis.filter(
  (id) => (!only || id === only) && uiJson(id).offline === true,
);
const precached = offline.filter(
  (id) => only || uiJson(id).precache_assembly !== false,
);
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
      const assets = fs.existsSync(assetsDir)
        ? fs
            .readdirSync(assetsDir)
            .sort()
            .map((f) => `assets/${f}`)
        : [];
      const capsManifest = path.join(outDir, "caps/manifest.json");
      const caps = fs.existsSync(capsManifest)
        ? JSON.parse(fs.readFileSync(capsManifest, "utf8"))
        : null;
      const shell = [
        ...offline.map((id) => `${id}.html`),
        ...assets,
        ...(caps ? ["caps/manifest.json"] : []),
        ...(caps?.assemblies ?? [])
          .filter((a) => precached.includes(a.ui))
          .map((a) => a.url),
      ];
      const hash = createHash("sha256");
      for (const file of shell)
        hash.update(file).update(fs.readFileSync(path.join(outDir, file)));
      const build = hash.digest("hex").slice(0, 16);
      const template = fs.readFileSync(
        path.join(root, "service-worker.js"),
        "utf8",
      );
      fs.writeFileSync(
        path.join(outDir, "sw.js"),
        `const BUILD = ${JSON.stringify(build)};\nconst SHELL = ${JSON.stringify(shell)};\nconst OFFLINE = ${JSON.stringify(offline)};\n\n${template}`,
      );
    },
  };
}

export default defineConfig({
  // Relative, so a deployment can serve the pages from any path, such as a
  // static host's prefix (/ride/).
  base: "./",
  plugins: [svelte(), ...(only ? [oneUi(only)] : []), serviceWorker()],
  build: only
    ? {
        target: "esnext",
        outDir: `dist-ui/${only}`,
        rollupOptions: { input: { [only]: `${only}.html` } },
      }
    : { target: "esnext", rollupOptions: { input } },
  publicDir: only ? false : "public",
  worker: { format: "es" },
  server: {
    // `npm run dev` proxies the sync server's routes (server/main.go).
    // `/rt` is the feed relay for feeds that send no CORS header
    // (`just dev` runs the server with -rt-proxy). A route that begins with
    // ^ is a pattern. `^/stops$` matches the document's writes and not the
    // stops page, `/stops.html`.
    proxy: Object.fromEntries(
      [
        "/rt",
        "/manifest",
        "/network",
        "/streets",
        "/walks",
        "/edits",
        "/pointers",
        "/blobs",
        "/permissions",
        "/plans",
        "/logs",
        "^/stops$",
        "^/stop-requests(/|$)",
        "/attachments",
        "/service-alerts",
        "/alert-templates",
        "^/notes$",
        "/feed",
        "/geocode",
      ].map(
        // TRANSIT_SERVER names another server (`just qa` runs its own).
        (route) => [
          route,
          process.env.TRANSIT_SERVER ?? "http://127.0.0.1:8080",
        ],
      ),
    ),
  },
});
