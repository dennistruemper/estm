import "./styles.css";
import { mount } from "svelte";
import App from "./App.svelte";

const root = document.getElementById("app");
if (root === null) {
  throw new Error('#app missing in index.html — Svelte bootstrap required');
}

mount(App, { target: root });
