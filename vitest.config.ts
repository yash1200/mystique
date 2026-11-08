import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vitest/config'

const appStub = new URL('./scripts/workspace/sveltekit-app-stub.ts', import.meta.url).pathname

export default defineConfig({
	// Feature barrels re-export components and rune stores, so a test that
	// imports one pure module can still pull in `.svelte` / `.svelte.ts`.
	plugins: [svelte({ configFile: false })],
	resolve: {
		conditions: ['browser'],
		alias: {
			'$app/state': appStub,
			'$app/navigation': appStub,
			'$app/paths': appStub,
		},
	},
	test: {
		environment: 'happy-dom',
		include: ['scripts/workspace/**/*.test.ts'],
	},
})
