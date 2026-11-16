#!/usr/bin/env node
/**
 * Set the app version everywhere it is written:
 *
 *   package.json               "version"
 *   src-tauri/Cargo.toml       [package] version
 *   src-tauri/tauri.conf.json  "version" (the bundle and About window)
 *   Cargo.lock                 the `mystique` package entry
 *
 * Run: `pnpm bump 1.0.2` (or `node scripts/bump-version.mjs 1.0.2`).
 * It edits the files only. Commit, tag and push yourself; a `v*` tag starts
 * the macOS release workflow.
 */
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/

/** Each file and the one version field in it: `before`, the version, `after`. */
const TARGETS = [
	{ file: 'package.json', pattern: /^(\t"version": ")([^"]+)(",)$/m },
	{ file: 'src-tauri/Cargo.toml', pattern: /^(\[package\]\nname = "mystique"\nversion = ")([^"]+)(")$/m },
	{ file: 'src-tauri/tauri.conf.json', pattern: /^(\t"version": ")([^"]+)(",)$/m },
	{ file: 'Cargo.lock', pattern: /^(\[\[package\]\]\nname = "mystique"\nversion = ")([^"]+)(")$/m },
]

function fail(message) {
	console.error(`bump-version: ${message}`)
	process.exit(1)
}

const next = process.argv[2]?.trim().replace(/^v/, '')
if (!next) fail('usage: pnpm bump <version>   e.g. pnpm bump 1.0.2')
if (!SEMVER.test(next)) fail(`"${next}" is not a semver version (x.y.z).`)

// Read and check every file before writing any, so a miss changes nothing.
const edits = TARGETS.map(({ file, pattern }) => {
	const path = join(ROOT, file)
	const text = readFileSync(path, 'utf8')
	const match = pattern.exec(text)
	if (!match) fail(`no version field found in ${file}.`)
	return {
		file,
		path,
		current: match[2],
		text: text.replace(pattern, (_, before, _old, after) => `${before}${next}${after}`),
	}
})

const versions = new Set(edits.map((e) => e.current))
if (versions.size > 1) {
	console.warn('bump-version: the files disagreed:')
	for (const e of edits) console.warn(`  ${e.file}: ${e.current}`)
}
if (versions.size === 1 && versions.has(next)) {
	console.log(`Already at ${next}. Nothing changed.`)
	process.exit(0)
}

for (const e of edits) {
	writeFileSync(e.path, e.text)
	console.log(`${e.file}: ${e.current} → ${next}`)
}
console.log(`\nNext: git commit -am "chore(release): version ${next}" && git tag -a v${next} -m "Mystique ${next}"`)
