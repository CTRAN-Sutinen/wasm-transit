import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  // wasm-bindgen glue uses top-level await
  build: { target: "esnext" },
  server: {
    // `npm run dev` proxies feed requests to transit-server, which owns
    // the /rt CORS proxy.
    proxy: { "/rt": "http://127.0.0.1:8080" },
  },
});
