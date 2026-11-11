import "../base.css";
import "maplibre-gl/dist/maplibre-gl.css";
import { mount } from "svelte";
import Shell from "./Shell.svelte";
import { boot } from "./host.svelte";

// The service-change UI mounts, then boots from the shared plan set (or
// the server's network when there is none). Mounting first paints the
// layout; each frame fills as its answer lands (the host's answer
// graph — TRANSIENT, docs/units/host-discipline.md).
mount(Shell, { target: document.getElementById("app")! });
await boot();
