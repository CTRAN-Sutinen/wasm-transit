import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  // wasm-bindgen glue uses top-level await
  build: { target: "esnext" },
  server: {
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
