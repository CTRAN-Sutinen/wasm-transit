import "../base.css";
import "maplibre-gl/dist/maplibre-gl.css";
import { mount } from "svelte";
import Shell from "./Shell.svelte";
import { boot } from "./host.svelte";

// The service-design UI boots from the shared plan set (or the server's feed
// when there is none) and mounts — one host, one layout.
await boot();
mount(Shell, { target: document.getElementById("app")! });
