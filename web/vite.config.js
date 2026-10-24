import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import path from "node:path";

export default defineConfig({
  plugins: [svelte()],
  // wasm-bindgen glue uses top-level await
  build: { target: "esnext" },
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
    },
  },
});
