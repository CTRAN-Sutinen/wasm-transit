import "../uis/base.css";
import { mount } from "svelte";
import Picker from "./Picker.svelte";

mount(Picker, { target: document.getElementById("app")! });
