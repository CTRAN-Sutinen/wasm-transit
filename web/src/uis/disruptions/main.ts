import "../base.css";
import "maplibre-gl/dist/maplibre-gl.css";
import { mount } from "svelte";
import Shell from "./Shell.svelte";
import { boot } from "./host.svelte";

// The disruptions UI mounts, then boots from the deployment's network,
// its documents, and the agency's live feeds. Mounting first paints the
// layout; each frame fills as its answer lands (the host's answer
// graph, src/uis/kit.ts).
mount(Shell, { target: document.getElementById("app")! });
await boot();
