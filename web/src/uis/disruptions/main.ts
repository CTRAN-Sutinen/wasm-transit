import "../base.css";
import "maplibre-gl/dist/maplibre-gl.css";
import { mount } from "svelte";
import Shell from "./Shell.svelte";
import { boot } from "./host.svelte";

// Mounting first paints the layout, and each frame fills as its answer
// lands.
mount(Shell, { target: document.getElementById("app")! });
await boot();
