import "../base.css";
import "maplibre-gl/dist/maplibre-gl.css";
import { mount } from "svelte";
import Shell from "./Shell.svelte";
import { boot } from "./host.svelte";

// The stops UI mounts, then boots from the deployment's stops document
// (or, before there is one, the stops of the network running today).
// Mounting first paints the layout; each frame fills as its answer
// lands (the host's answer graph — TRANSIENT,
// docs/units/host-discipline.md).
mount(Shell, { target: document.getElementById("app")! });
await boot();
