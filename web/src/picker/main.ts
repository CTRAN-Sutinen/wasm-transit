import "../uis/base.css";
import { mount } from "svelte";
import Picker from "./Picker.svelte";

// The front door mounts the picker; each UI is its own page (its own
// assembly, its own state) one link away.
mount(Picker, { target: document.getElementById("app")! });
