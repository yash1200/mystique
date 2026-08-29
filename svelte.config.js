// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({
      fallback: "index.html",
    }),
    alias: {
      // Zones — see docs/COMPONENT_STRUCTURE.md. Direction is
      // routes → features → engines → core, enforced by eslint boundaries.
      $core: "src/lib/core",
      $engines: "src/lib/engines",
      $features: "src/lib/features",
      $icons: "src/assets/icons",
      $illustrations: "src/assets/illustrations",
    },
  },
};

export default config;
