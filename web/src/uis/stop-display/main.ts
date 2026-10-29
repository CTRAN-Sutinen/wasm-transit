import "../base.css";
import { mount } from "svelte";
import Shell from "./Shell.svelte";
import { boot } from "./host.svelte";

// The stop display mounts, then boots from the server's published feed
// — one host, three layouts. Mounting first paints the layout and the
// boot's status at once; each frame fills as its answer lands (the
// host's answer graph — TRANSIENT, the engine's to replace:
// docs/units/host-discipline.md).
mount(Shell, { target: document.getElementById("app")! });
await boot();
