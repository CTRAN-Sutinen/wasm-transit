import "../base.css";
import "maplibre-gl/dist/maplibre-gl.css";
import { mount } from "svelte";
import Shell from "./Shell.svelte";
import { boot } from "./host.svelte";

// The service-design UI mounts, then boots from the shared plan set (or
// the server's feed when there is none) — one host, one layout. Mounting
// first paints the layout and the boot's status at once; each frame
// fills as its answer lands (the host's answer graph — TRANSIENT, the
// engine's to replace: docs/units/host-discipline.md).
mount(Shell, { target: document.getElementById("app")! });
await boot();
