import "./app.css";
import { mount } from "svelte";
import "maplibre-gl/dist/maplibre-gl.css";
// The wasm artifact has zero imports — the wrapper (src/wasm.js)
// fetches it and instantiates it directly, then exposes the same
// GtfsHandle API the app speaks (the rt decoder lives in cap-rt).
import { loadWasm } from "./wasm";
import { get } from "idb-keyval";
import App from "./App.svelte";

// E2E/inspection hooks.
declare global {
  interface Window {
    __setNow: (v: { date: string; secs: number } | null) => void;
    __flushEdits: () => Promise<void>;
    __clearEdits: () => void;
    __map: unknown;
    /// Which path served the last planner analysis refresh: the
    /// capability bus ("bus") or the monolith fallback.
    __analysisPath: () => "bus" | "monolith" | null;
    /// Drop a loaded capability so the next `ensure` re-fetches it —
    /// the degrade simulation for e2e (the resolver's own lever).
    __forgetCapability: (id: string) => void;
  }
}
import { app, setEditDocument, restorePlans } from "./state.svelte";
import { analysisPath } from "./planner";
import { loadGtfsBytes, FEED_KEY, EDITS_KEY, saveSettings, setNowOverride, flushPersist, syncEdits } from "./actions";
import { forgetCapability } from "./capabilities.svelte";

app.wasm = await loadWasm();
app.status = "wasm ready";

// E2E/inspection hooks: override the app's clock, read the map state
// (set by MapView), force the debounced edit-document write so
// reloads in tests are deterministic, and drop the local edit
// document (so a test can isolate itself from the server's shared
// one). All no-op in normal use.
window.__setNow = setNowOverride;
window.__analysisPath = () => analysisPath;
window.__forgetCapability = forgetCapability;
window.__flushEdits = flushPersist;
window.__clearEdits = () => {
  setEditDocument([]);
  flushPersist();
};

// Bootstrap, in dependency order: settings (cheap, local) → the edit
// document (plain data, may exist without a feed) → the cached feed
// bytes (loading it re-pushes the document so every record is
// revalidated against the artifact). One path, no interleaving.
try {
  const settings = JSON.parse(localStorage.getItem("transit:settings") ?? "{}");
  app.gtfsUrl = settings.gtfsUrl ?? "";
  app.rtUrl = settings.rtUrl ?? "";
  app.vpUrl = settings.vpUrl ?? "";
  app.streetsUrl = settings.streetsUrl ?? "";
  app.rtPolling = !!settings.rtPolling && !!settings.rtUrl;
  app.vpPolling = !!settings.vpPolling && !!settings.vpUrl;
  app.persona = (localStorage.getItem("transit:persona") ?? "rider") as "rider";

  await restorePlans();
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

mount(App, { target: document.getElementById("app")! });
