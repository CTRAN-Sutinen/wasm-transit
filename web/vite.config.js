import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import path from "node:path";
import fs from "node:fs";
import { fileURLToPath } from "node:url";

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

export default defineConfig({
  plugins: [svelte()],
  build: { target: "esnext", rollupOptions: { input } },
  // The bus worker (src/bus.worker.ts) is a module worker.
  worker: { format: "es" },
  server: {
    // `npm run dev` proxies the sync server's routes (server/main.go):
    // the published documents, the manifest, edit sync and the store,
    // the geocoder, and the /rt feed proxy (dev only, until the
    // agency's feeds serve HTTPS with CORS — docs/units/deploy-rider-apps.md).
    proxy: Object.fromEntries(
      ["/rt", "/manifest", "/network", "/streets", "/walks", "/edits", "/pointers", "/blobs", "/permissions", "/geocode"].map(
        (route) => [route, "http://127.0.0.1:8080"],
      ),
    ),
  },
});
