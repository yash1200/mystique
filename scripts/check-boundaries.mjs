#!/usr/bin/env node
/**
 * Zone boundary and import-cycle check for the frontend.
 *
 * Direction is routes → features → engines → core. A feature may use another
 * feature only through its public `index.ts`, never by reaching into its files.
 *
 * Workspace has a kernel and a shell:
 * - `workspace/session` is the kernel. Every feature may import it (through
 *   its index). It must not import UI: no `.svelte` files, nothing under a
 *   `ui/` folder, and never the shell.
 * - `workspace/shell` is composition only. Nothing imports it except routes
 *   (and the `#features/workspace` aggregate, which routes mount).
 *
 * The module graph over src/lib + src/routes must have no import cycles.
 * Type-only imports (`import type …`, `export type … from`) are erased at
 * build time, so they do not count as cycle edges. Dynamic `import()` is lazy
 * and does not count either.
 *
 * Run with `pnpm check:boundaries` (also part of `pnpm check`).
 */
import { readFileSync, existsSync, statSync } from 'node:fs'
import { execSync } from 'node:child_process'
import path from 'node:path'
import process from 'node:process'

const ROOT = process.cwd()
const LIB = path.join(ROOT, 'src/lib')
const ROUTES = path.join(ROOT, 'src/routes')

/**
 * Imports known to close a cycle that is out of scope to break right now.
 * Each entry is one edge (`from` imports `to`, paths relative to the repo
 * root); it is left out of the cycle graph. Keep this empty if you can, and
 * say why for every entry.
 */
const CYCLE_ALLOWLIST = []

const KERNEL = 'workspace/session'
const SHELL = 'workspace/shell'
/** The aggregate `#features/workspace` re-exports the shell for routes. */
const AGGREGATE = 'workspace'

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
	if (spec.startsWith('#core/') || spec === '#core') {
		return { kind: 'core', id: 'core', deep: false }
	}
	if (spec === '#engines' || spec.startsWith('#engines/')) {
		return { kind: 'engines', id: 'engines', deep: spec !== '#engines' }
	}
	if (spec.startsWith('#lib/app')) return { kind: 'app', id: 'app', deep: false }
	if (spec.startsWith('#features/')) {
		const rest = spec.slice('#features/'.length).replace(/\/index\.js$/, '')
		const m = rest.match(/^(workspace\/[^/]+|[^/]+)(\/.*)?$/)
		if (!m) return null
		return { kind: 'feature', id: m[1], deep: !!m[2] }
	}
	if (spec.startsWith('.')) {
		const c = resolveFile(path.resolve(path.dirname(fromFile), spec))
		if (!c) return null
		const z = zoneOf(c)
		if (!z) return null
		// A zone's own index.ts is its public surface, however you reach it.
		const isBarrel =
			z.kind === 'feature' &&
			path.relative(LIB, c) === path.join('features', z.id, 'index.ts')
		return { ...z, deep: !isBarrel }
	}
	return null
}

/** The file a path without extension (or with `.js`) stands for. */
function resolveFile(base) {
	const bare = base.replace(/\.js$/, '')
	for (const c of [
		bare + '.ts',
		bare + '.svelte',
		bare + '.svelte.ts',
		path.join(bare, 'index.ts'),
		base,
	]) {
		if (existsSync(c) && !statSync(c).isDirectory()) return c
	}
	return null
}

/** package.json `imports`: exact keys first, then `#x/*` patterns. */
const pkgImports = JSON.parse(readFileSync(path.join(ROOT, 'package.json'), 'utf8')).imports ?? {}

/** Absolute file an import specifier resolves to, or null when external. */
function resolveSpec(fromFile, spec) {
	if (spec.startsWith('.')) return resolveFile(path.resolve(path.dirname(fromFile), spec))
	if (spec === '$lib' || spec.startsWith('$lib/')) {
		return resolveFile(path.join(LIB, spec.slice('$lib'.length)))
	}
	if (!spec.startsWith('#')) return null
	if (pkgImports[spec]) return resolveFile(path.resolve(ROOT, pkgImports[spec]))
	let best = null
	for (const [key, target] of Object.entries(pkgImports)) {
		if (!key.endsWith('/*')) continue
		const prefix = key.slice(0, -1)
		if (!spec.startsWith(prefix)) continue
		if (!best || prefix.length > best.prefix.length) best = { prefix, target }
	}
	if (!best) return null
	const rest = spec.slice(best.prefix.length)
	return resolveFile(path.resolve(ROOT, best.target.replace('*', rest)))
}

/**
 * Static imports and re-exports of a file. `typeOnly` is true for
 * `import type …` and `export type … from`. The clause between the keyword and
 * `from` is limited to identifier characters so a match cannot run into the
 * next statement.
 */
function importsOf(src) {
	const out = []
	const re = /\b(import|export)\s+(type\s+)?([\w$*{},\s]*?)\s*from\s*'([^']+)'/g
	for (const m of src.matchAll(re)) {
		out.push({ spec: m[4], typeOnly: !!m[2] })
	}
	for (const m of src.matchAll(/\bimport\s+'([^']+)'/g)) {
		out.push({ spec: m[1], typeOnly: false })
	}
	return out
}

const RANK = { core: 0, engines: 1, app: 2, feature: 3, routes: 4 }

const files = execSync(
	`find ${ROOT}/src -type f \\( -name '*.ts' -o -name '*.svelte' \\)`,
	{ encoding: 'utf8' },
)
	.trim()
	.split('\n')

const violations = []
/** Runtime import graph: file → set of files. */
const graph = new Map()
const allowed = new Set(CYCLE_ALLOWLIST.map((e) => `${e.from}\0${e.to}`))
const usedAllow = new Set()

for (const file of files) {
	const inGraph = file.startsWith(LIB + path.sep) || file.startsWith(ROUTES + path.sep)
	if (inGraph) graph.set(file, new Set())
	const from = zoneOf(file)
	const src = readFileSync(file, 'utf8')
	for (const { spec, typeOnly } of importsOf(src)) {
		if (inGraph && !typeOnly) {
			const dest = resolveSpec(file, spec)
			const key = `${path.relative(ROOT, file)}\0${dest && path.relative(ROOT, dest)}`
			if (dest && allowed.has(key)) usedAllow.add(key)
			else if (dest) graph.get(file).add(dest)
		}

		if (!from) continue
		const where = `${path.relative(ROOT, file)} → ${spec}`

		// The session kernel stays free of UI, whatever zone it reaches into.
		if (from.kind === 'feature' && from.id === KERNEL) {
			const dest = resolveSpec(file, spec)
			const rel = dest ? path.relative(LIB, dest) : ''
			if (dest && (dest.endsWith('.svelte') || rel.split(path.sep).includes('ui'))) {
				violations.push(`${where}\n    the session kernel must not import UI`)
				continue
			}
		}

		const to = targetOf(file, spec)
		if (!to) continue

		// Same zone: anything goes.
		if (from.kind === to.kind && from.id === to.id) continue

		// Never import upward.
		if (RANK[to.kind] > RANK[from.kind]) {
			violations.push(`${where}\n    ${from.kind} must not import ${to.kind}`)
			continue
		}

		// The shell is composition only: routes mount it, nothing else imports it.
		if (to.kind === 'feature' && (to.id === SHELL || to.id === AGGREGATE)) {
			const parentReexport = from.kind === 'feature' && from.id === AGGREGATE
			if (from.kind !== 'routes' && !parentReexport) {
				violations.push(
					`${where}\n    only routes may import the workspace shell; use '#features/${KERNEL}'`,
				)
				continue
			}
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

		// Routes use a feature's index, except to mount a page (`ui/*Page.svelte`)
		// that composes several features and so stays out of the index.
		if (from.kind === 'routes' && to.kind === 'feature' && to.deep) {
			if (!/\/ui\/[A-Z]\w*Page\.svelte$/.test(spec)) {
				violations.push(
					`${where}\n    routes reach a feature through '#features/${to.id}' or mount its ui/*Page.svelte`,
				)
			}
			continue
		}

		// Cross-feature: public surface only.
		if (from.kind === 'feature' && to.kind === 'feature' && to.deep) {
			violations.push(
				`${where}\n    reach another feature through '#features/${to.id}', not its files`,
			)
			continue
		}

		// Engines are one module to everyone outside them.
		if (from.kind !== 'engines' && to.kind === 'engines' && to.deep) {
			violations.push(`${where}\n    import '#engines', not an engine folder`)
		}
	}
}

/** Strongly connected components (Tarjan), largest first. */
function stronglyConnected(g) {
	let index = 0
	const idx = new Map()
	const low = new Map()
	const onStack = new Set()
	const stack = []
	const out = []
	for (const start of g.keys()) {
		if (idx.has(start)) continue
		// Iterative DFS so deep graphs cannot overflow the call stack.
		const work = [{ v: start, it: null }]
		while (work.length) {
			const frame = work[work.length - 1]
			const { v } = frame
			if (frame.it === null) {
				idx.set(v, index)
				low.set(v, index)
				index++
				stack.push(v)
				onStack.add(v)
				frame.it = (g.get(v) ?? new Set()).values()
			}
			let pushed = false
			for (let n = frame.it.next(); !n.done; n = frame.it.next()) {
				const w = n.value
				if (!g.has(w)) continue
				if (!idx.has(w)) {
					work.push({ v: w, it: null })
					pushed = true
					break
				}
				if (onStack.has(w)) low.set(v, Math.min(low.get(v), idx.get(w)))
			}
			if (pushed) continue
			work.pop()
			if (work.length) {
				const parent = work[work.length - 1].v
				low.set(parent, Math.min(low.get(parent), low.get(v)))
			}
			if (low.get(v) === idx.get(v)) {
				const comp = []
				let w
				do {
					w = stack.pop()
					onStack.delete(w)
					comp.push(w)
				} while (w !== v)
				const selfLoop = comp.length === 1 && g.get(v)?.has(v)
				if (comp.length > 1 || selfLoop) out.push(comp)
			}
		}
	}
	return out.sort((a, b) => b.length - a.length)
}

/** Shortest cycle through `start` inside one component, as a closed path. */
function shortestCycle(g, start, members) {
	const prev = new Map([[start, null]])
	const queue = [start]
	while (queue.length) {
		const v = queue.shift()
		for (const w of g.get(v) ?? []) {
			if (!members.has(w)) continue
			if (w === start) {
				const pathOut = [start]
				for (let x = v; x !== start; x = prev.get(x)) pathOut.splice(1, 0, x)
				return [...pathOut, start]
			}
			if (!prev.has(w)) {
				prev.set(w, v)
				queue.push(w)
			}
		}
	}
	return null
}

const cycles = []
for (const comp of stronglyConnected(graph)) {
	const members = new Set(comp)
	const seen = new Set()
	for (const v of [...comp].sort()) {
		const cyc = shortestCycle(graph, v, members)
		if (!cyc) continue
		// Same cycle from another start: compare by its rotation to the smallest node.
		const ring = cyc.slice(0, -1)
		const min = ring.indexOf([...ring].sort()[0])
		const canon = [...ring.slice(min), ...ring.slice(0, min)].join('\0')
		if (seen.has(canon)) continue
		seen.add(canon)
		cycles.push({ size: comp.length, path: cyc.map((f) => path.relative(ROOT, f)) })
	}
}

// An allowlist entry whose import is gone is stale: drop it.
for (const e of CYCLE_ALLOWLIST) {
	if (!usedAllow.has(`${e.from}\0${e.to}`)) {
		violations.push(`${e.from} → ${e.to}\n    stale CYCLE_ALLOWLIST entry: no such import`)
	}
}

let failed = false
if (violations.length) {
	failed = true
	console.error(`\n${violations.length} boundary violation(s):\n`)
	for (const v of violations) console.error(`  ${v}\n`)
}
if (cycles.length) {
	failed = true
	console.error(`\n${cycles.length} import cycle(s) (type-only imports excluded):\n`)
	for (const c of cycles) {
		console.error(`  [group of ${c.size} modules]\n    ${c.path.join('\n    → ')}\n`)
	}
}
if (failed) process.exit(1)
console.log('boundaries ok, no import cycles')
