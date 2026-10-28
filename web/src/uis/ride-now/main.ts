import "../base.css";
import "maplibre-gl/dist/maplibre-gl.css";
import { mount } from "svelte";
import Shell from "./Shell.svelte";
import { boot } from "./host.svelte";

// The ride-now UI mounts, then boots from the server's published feed
// (or a zip the rider loads) — one host, one layout. Mounting first
// paints the layout and the boot's status at once; each frame fills as
// its answer lands (the host's answer graph — TRANSIENT, the engine's
// to replace: docs/units/host-discipline.md).
mount(Shell, { target: document.getElementById("app")! });
await boot();
