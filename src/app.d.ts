// Svelte ambient types (vite-plugin-svelte also augments vite/client).

declare module "*.svelte" {
  import type { Component as SvelteComponent } from "svelte";
  /* eslint-disable @typescript-eslint/no-explicit-any */
  const cmp: typeof SvelteComponent<any, any>;
  export default cmp;
}
