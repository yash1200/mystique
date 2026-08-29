#!/usr/bin/env node
/**
 * Zone boundary check for the frontend.
 *
 * Direction is routes → features → engines → core. A feature may use another
 * feature only through its public `index.ts`, never by reaching into its files.
 * Run with `npm run check:boundaries` (also part of `npm run check`).
 */
import { readFileSync, existsSync, statSync } from 'node:fs'
import { execSync } from 'node:child_process'
import path from 'node:path'
import process from 'node:process'

const ROOT = process.cwd()
const LIB = path.join(ROOT, 'src/lib')

/** Which zone a file belongs to. */
function zoneOf(file) {
	const rel = path.relative(ROOT, file)
	if (rel.startsWith('src/routes/')) return { kind: 'routes', id: 'routes' }
	const lib = path.relative(LIB, file)
	if (lib.startsWith('..')) return null
	if (lib.startsWith('core/')) return { kind: 'core', id: 'core' }
	if (lib.startsWith('engines/')) return { kind: 'engines', id: 'engines' }
	if (lib.startsWith('app/')) return { kind: 'app', id: 'app' }
	const m = lib.match(/^features\/(workspace\/[^/]+|[^/]+)\//)
	if (m) return { kind: 'feature', id: m[1] }
	return null
}

/** Which zone an import specifier points at, and whether it is a deep import. */
function targetOf(fromFile, spec) {
	if (spec.startsWith('$core')) return { kind: 'core', id: 'core', deep: false }
	if (spec.startsWith('$engines')) {
		return { kind: 'engines', id: 'engines', deep: spec !== '$engines' }
	}
	if (spec.startsWith('$lib/app')) return { kind: 'app', id: 'app', deep: false }
	if (spec.startsWith('$features/')) {
		const rest = spec.slice('$features/'.length)
		const m = rest.match(/^(workspace\/[^/]+|[^/]+)(\/.*)?$/)
		if (!m) return null
		return { kind: 'feature', id: m[1], deep: !!m[2] }
	}
	if (spec.startsWith('.')) {
		const base = path.resolve(path.dirname(fromFile), spec)
		for (const c of [
			base + '.ts',
			base + '.svelte',
			base + '.svelte.ts',
			path.join(base, 'index.ts'),
			base,
		]) {
			if (existsSync(c) && !statSync(c).isDirectory()) {
				const z = zoneOf(c)
				if (!z) return null
				// A zone's own index.ts is its public surface, however you reach it.
				const isBarrel =
					z.kind === 'feature' &&
					path.relative(LIB, c) === path.join('features', z.id, 'index.ts')
				return { ...z, deep: !isBarrel }
			}
		}
	}
	return null
}

const RANK = { core: 0, engines: 1, app: 2, feature: 3, routes: 4 }

const files = execSync(
	`find ${ROOT}/src -type f \\( -name '*.ts' -o -name '*.svelte' \\)`,
	{ encoding: 'utf8' },
)
	.trim()
	.split('\n')

const violations = []
for (const file of files) {
	const from = zoneOf(file)
	if (!from) continue
	const src = readFileSync(file, 'utf8')
	const specs = [...src.matchAll(/\bfrom\s+'([^']+)'/g)].map((m) => m[1])
	for (const spec of specs) {
		const to = targetOf(file, spec)
		if (!to) continue
		const where = `${path.relative(ROOT, file)} → ${spec}`

		// Same zone: anything goes.
		if (from.kind === to.kind && from.id === to.id) continue

		// Never import upward.
		if (RANK[to.kind] > RANK[from.kind]) {
			violations.push(`${where}\n    ${from.kind} must not import ${to.kind}`)
			continue
		}

		// A parent aggregate (features/workspace) may re-export its own children.
		if (
			from.kind === 'feature' &&
			to.kind === 'feature' &&
			to.id.startsWith(`${from.id}/`) &&
			!to.deep
		) {
			continue
		}

		// Cross-feature: public surface only.
		if (from.kind === 'feature' && to.kind === 'feature' && to.deep) {
			violations.push(
				`${where}\n    reach another feature through '$features/${to.id}', not its files`,
			)
			continue
		}

		// Engines are one module to everyone outside them.
		if (from.kind !== 'engines' && to.kind === 'engines' && to.deep) {
			violations.push(`${where}\n    import '$engines', not an engine folder`)
		}
	}
}

if (violations.length) {
	console.error(`\n${violations.length} boundary violation(s):\n`)
	for (const v of violations) console.error(`  ${v}\n`)
	process.exit(1)
}
console.log('boundaries ok')
