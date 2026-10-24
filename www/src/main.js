import "./app.css";
import { mount } from "svelte";
import "maplibre-gl/dist/maplibre-gl.css";
// wasm-bindgen 0.2.129 bundler output uses synchronous ESM integration:
// importing the module loads the wasm — no init()/fetch dance.
import { GtfsHandle, decode_rt } from "../pkg/transit_wasm.js";
import { get } from "idb-keyval";
import App from "./App.svelte";
import { app } from "./state.svelte.js";
import { loadGtfsBytes, FEED_KEY, saveSettings, setNowOverride } from "./actions.svelte.js";

app.wasm = { GtfsHandle, decode_rt };
app.status = "wasm ready";

// E2E/inspection hooks: override the app's clock and read the map
// state (set by MapView). Both no-op in normal use.
window.__setNow = setNowOverride;

// Restore the cached feed (bytes in IndexedDB) and settings — refresh
// should land you back where you were, offline, without re-downloading.
try {
  const cached = await get(FEED_KEY);
  if (cached) {
    loadGtfsBytes(new Uint8Array(cached.bytes), cached.label);
    app.status = `Restored ${cached.label} from local cache.`;
  }
  const settings = JSON.parse(localStorage.getItem("transit:settings") ?? "{}");
  app.gtfsUrl = settings.gtfsUrl ?? "";
  app.rtUrl = settings.rtUrl ?? "";
  app.vpUrl = settings.vpUrl ?? "";
  app.rtPolling = !!settings.rtPolling && !!settings.rtUrl;
  app.vpPolling = !!settings.vpPolling && !!settings.vpUrl;
  // Polling effects in App pick up the restored toggles from here.
  saveSettings();
} catch (e) {
  console.warn("restore failed:", e);
  app.status = "wasm ready — drop a GTFS zip or paste a URL to begin.";
}

mount(App, { target: document.getElementById("app") });
