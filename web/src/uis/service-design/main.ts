import "../base.css";
import "maplibre-gl/dist/maplibre-gl.css";
import { mount } from "svelte";
import Shell from "./Shell.svelte";
import { boot } from "./host.svelte";

// Mounting first paints the layout while the boot runs.
mount(Shell, { target: document.getElementById("app")! });
await boot();
