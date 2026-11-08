import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vitest/config'

export default defineConfig({
	// Feature barrels re-export components and rune stores, so a test that
	// imports one pure module can still pull in `.svelte` / `.svelte.ts`.
	plugins: [svelte()],
	resolve: { conditions: ['browser'] },
	test: {
		environment: 'happy-dom',
		include: ['scripts/workspace/**/*.test.ts'],
	},
})
