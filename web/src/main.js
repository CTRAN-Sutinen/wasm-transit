import "./app.css";
import { mount } from "svelte";
import "maplibre-gl/dist/maplibre-gl.css";
// The wasm artifact has zero imports — the wrapper (src/wasm.js)
// fetches it and instantiates it directly, then exposes the same
// GtfsHandle/decode_rt API the app speaks.
import { loadWasm } from "./wasm.js";
import { get } from "idb-keyval";
import App from "./App.svelte";
import { app, setEditDocument } from "./state.svelte.js";
import { loadGtfsBytes, FEED_KEY, EDITS_KEY, saveSettings, setNowOverride, flushPersist, syncEdits } from "./actions.svelte.js";

app.wasm = await loadWasm();
app.status = "wasm ready";

// E2E/inspection hooks: override the app's clock, read the map state
// (set by MapView), and force the debounced edit-document write so
// reloads in tests are deterministic. All no-op in normal use.
window.__setNow = setNowOverride;
window.__flushEdits = flushPersist;

// Bootstrap, in dependency order: settings (cheap, local) → the edit
// document (plain data, may exist without a feed) → the cached feed
// bytes (loading it re-pushes the document so every record is
// revalidated against the artifact). One path, no interleaving.
try {
  const settings = JSON.parse(localStorage.getItem("transit:settings") ?? "{}");
  app.gtfsUrl = settings.gtfsUrl ?? "";
  app.rtUrl = settings.rtUrl ?? "";
  app.vpUrl = settings.vpUrl ?? "";
  app.rtPolling = !!settings.rtPolling && !!settings.rtUrl;
  app.vpPolling = !!settings.vpPolling && !!settings.vpUrl;

  const edits = await get(EDITS_KEY);
  if (Array.isArray(edits)) setEditDocument(edits);

  const cached = await get(FEED_KEY);
  if (cached) {
    // Awaited: the final status is deterministic ("Restored", not a
    // race with loadGtfsBytes's "Loaded").
    await loadGtfsBytes(new Uint8Array(cached.bytes), cached.label);
  }
  // Pull the server's edit document (quietly — status stays with the
  // restore message; a server without /edits is not an error).
  await syncEdits({ quiet: true });
  if (cached) {
    app.status = `Restored ${cached.label} from local cache.`;
  }

  // Polling effects in App pick up the restored toggles from here.
  saveSettings();
} catch (e) {
  console.warn("restore failed:", e);
  app.status = "wasm ready — drop a GTFS zip or paste a URL to begin.";
}

mount(App, { target: document.getElementById("app") });
