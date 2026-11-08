import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vitest/config'

export default defineConfig({
	// Compiles runes in `.svelte.ts` stores so tests can drive them directly.
	plugins: [svelte()],
	test: {
		environment: 'happy-dom',
		include: ['scripts/workspace/**/*.test.ts'],
	},
})
