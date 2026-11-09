#!/usr/bin/env node
/**
 * Crate layering check for the Rust workspace (#198).
 *
 * Dependencies point down the table, never up or sideways:
 *
 *   mystique (src-tauri shell), mystique-ipc-gen (writes the TS bindings)
 *     └ mystique-acp                        external agent (ACP) in the agent pane; reuses
 *                                           mystique-agent's turns, reaches tools via MCP
 *     └ mystique-mcp, mystique-agent        adapters over core
 *         └ mystique-core                   app services, storage, connection manager
 *             └ mystique-plugin-*           engine drivers (never each other, never core)
 *                 └ mystique-connector-api  plugin SPI (re-exports the three below)
 *                     └ mystique-transfer   import parsers, export writers
 *                     └ mystique-tls        TLS settings; no workspace deps
 *                     └ mystique-sql        pure SQL builders; no workspace deps
 *
 * Every workspace crate must match a rule. Normal, build and dev dependencies
 * are all checked. Non-workspace crates (crates.io) are not.
 *
 * Run: `node scripts/check-crate-layers.mjs` (needs `cargo` on PATH), or pass
 * saved `cargo metadata --format-version 1 --no-deps` output with
 * `--metadata <file>`.
 */
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import process from 'node:process'

const PLUGINS = 'mystique-plugin-*'

/**
 * crate (or `prefix-*`)   → workspace crates it may depend on.
 * Keep this table readable: it is the architecture.
 */
const RULES = [
	['mystique-sql', []],
	['mystique-tls', []],
	['mystique-transfer', ['mystique-sql']],
	['mystique-connector-api', ['mystique-sql', 'mystique-tls', 'mystique-transfer']],
	[PLUGINS, ['mystique-sql', 'mystique-connector-api', 'mystique-tls', 'mystique-transfer']],
	[
		'mystique-core',
		['mystique-sql', 'mystique-connector-api', 'mystique-tls', 'mystique-transfer', PLUGINS],
	],
	['mystique-mcp', ['mystique-core', 'mystique-connector-api', 'mystique-sql']],
	['mystique-agent', ['mystique-core', 'mystique-connector-api', 'mystique-sql']],
	// mystique-mcp only as a dev dependency in practice (the end-to-end tests
	// run the real MCP server); the shell hands the MCP endpoint in.
	['mystique-acp', ['mystique-core', 'mystique-agent', 'mystique-mcp']],
	[
		'mystique-ipc-gen',
		['mystique-core', 'mystique-mcp', 'mystique-agent', 'mystique-acp', 'mystique-connector-api'],
	],
	[
		'mystique',
		[
			'mystique-core',
			'mystique-mcp',
			'mystique-agent',
			'mystique-acp',
			'mystique-connector-api',
			'mystique-sql',
		],
	],
]

function matches(pattern, name) {
	return pattern.endsWith('*') ? name.startsWith(pattern.slice(0, -1)) : pattern === name
}

function ruleFor(name) {
	return RULES.find(([pattern]) => matches(pattern, name))
}

function loadMetadata() {
	const flag = process.argv.indexOf('--metadata')
	if (flag !== -1) return JSON.parse(readFileSync(process.argv[flag + 1], 'utf8'))
	const out = execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
		encoding: 'utf8',
		maxBuffer: 64 * 1024 * 1024,
	})
	return JSON.parse(out)
}

const metadata = loadMetadata()
const members = new Set(metadata.workspace_members)
const packages = metadata.packages.filter((p) => members.has(p.id))
const names = new Set(packages.map((p) => p.name))

const errors = []
for (const pkg of packages) {
	const rule = ruleFor(pkg.name)
	if (!rule) {
		errors.push(`${pkg.name}: no layering rule; add it to RULES in scripts/check-crate-layers.mjs`)
		continue
	}
	const [, allowed] = rule
	for (const dep of pkg.dependencies) {
		if (!names.has(dep.name)) continue
		if (dep.name === pkg.name) continue
		if (allowed.some((pattern) => matches(pattern, dep.name))) continue
		const kind = dep.kind ?? 'normal'
		errors.push(`${pkg.name} → ${dep.name} (${kind}) is not allowed; ${pkg.name} may use: ${allowed.join(', ') || 'no workspace crates'}`)
	}
}

if (errors.length) {
	console.error('crate layering check failed:')
	for (const e of errors) console.error(`  ${e}`)
	process.exit(1)
}
console.log(`crate layers ok (${packages.length} crates)`)
