import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import path from "node:path";
import fs from "node:fs";
import { fileURLToPath } from "node:url";

// Pages: the picker (/), the original app, and every generated UI —
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
  app: "app.html",
  ...Object.fromEntries(uis.map((id) => [id, `${id}.html`])),
};

export default defineConfig({
  plugins: [svelte()],
  // wasm-bindgen glue uses top-level await
  build: { target: "esnext", rollupOptions: { input } },
  server: {
    // The planner persona imports the shared network fixture straight
    // from the repo's testdata/ (one source of truth, also used by the
    // corpus and native tests) — the dev server needs to serve it.
    fs: { allow: [path.resolve("..")] },
    // `npm run dev` proxies API requests to the sync server, which
    // owns the /rt CORS proxy, the manifest, and edit sync.
    proxy: {
      "/rt": "http://127.0.0.1:8080",
      "/manifest": "http://127.0.0.1:8080",
      "/feed": "http://127.0.0.1:8080",
      "/edits": "http://127.0.0.1:8080",
      "/geocode": "http://127.0.0.1:8080",
    },
  },
});
