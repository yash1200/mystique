#!/usr/bin/env node
/**
 * Build a Tauri updater manifest from signed bundle artifacts.
 *
 *   node scripts/write-latest-json.mjs \
 *     --base-url https://example.com/releases/v0.2.0 \
 *     --notes "What changed" \
 *     --out latest.json
 */
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { basename, join } from 'node:path'
import process from 'node:process'

const args = parseArgs(process.argv.slice(2))
const root = process.cwd()
const version = readVersion(root)
const baseUrl = String(args['base-url'] ?? '').replace(/\/$/, '')
if (!baseUrl) {
	console.error('Missing --base-url (public directory that will host the bundles)')
	process.exit(1)
}

const platforms = {}
addPlatform(platforms, 'darwin-aarch64', firstExisting(root, [
	'target/release/bundle/macos/Mystique.app.tar.gz',
	'src-tauri/target/release/bundle/macos/Mystique.app.tar.gz',
	'target/aarch64-apple-darwin/release/bundle/macos/Mystique.app.tar.gz',
]))
addPlatform(platforms, 'darwin-x86_64', firstExisting(root, [
	'target/x86_64-apple-darwin/release/bundle/macos/Mystique.app.tar.gz',
	'src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Mystique.app.tar.gz',
]))
addPlatform(
	platforms,
	'windows-x86_64',
	firstExisting(root, [
		...listDir(root, 'target/x86_64-pc-windows-msvc/release/bundle/nsis', '.exe'),
		...listDir(root, 'target/release/bundle/nsis', '.exe'),
		...listDir(root, 'target/x86_64-pc-windows-msvc/release/bundle/msi', '.msi'),
	]),
)
addPlatform(
	platforms,
	'linux-x86_64',
	firstExisting(root, [
		...listDir(root, 'target/release/bundle/appimage', '.AppImage'),
		...listDir(root, 'src-tauri/target/release/bundle/appimage', '.AppImage'),
	]),
)

if (Object.keys(platforms).length === 0) {
	console.error('No updater artifacts found. Build with TAURI_SIGNING_PRIVATE_KEY set.')
	process.exit(1)
}

const manifest = {
	version,
	notes: typeof args.notes === 'string' ? args.notes : '',
	pub_date: new Date().toISOString(),
	platforms,
}

const out = typeof args.out === 'string' ? args.out : 'latest.json'
writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`)
console.log(`Wrote ${out} (${Object.keys(platforms).join(', ')})`)

function addPlatform(map, key, file) {
	if (!file) return
	const sig = `${file}.sig`
	if (!existsSync(sig)) {
		console.error(`Missing signature: ${sig}`)
		process.exit(1)
	}
	map[key] = {
		url: `${baseUrl}/${basename(file)}`,
		signature: readFileSync(sig, 'utf8').trim(),
	}
}

function firstExisting(base, rels) {
	for (const rel of rels) {
		const abs = join(base, rel)
		if (existsSync(abs)) return abs
	}
	return null
}

function listDir(base, dir, ext) {
	const abs = join(base, dir)
	if (!existsSync(abs)) return []
	return readdirSync(abs)
		.filter((name) => name.endsWith(ext) && !name.endsWith('.sig'))
		.map((name) => join(dir, name))
}

function parseArgs(argv) {
	/** @type {Record<string, string | true>} */
	const out = {}
	for (let i = 0; i < argv.length; i++) {
		const key = argv[i]
		if (!key.startsWith('--')) continue
		const next = argv[i + 1]
		if (next && !next.startsWith('--')) {
			out[key.slice(2)] = next
			i += 1
		} else {
			out[key.slice(2)] = true
		}
	}
	return out
}

function readVersion(base) {
	const conf = JSON.parse(readFileSync(join(base, 'src-tauri/tauri.conf.json'), 'utf8'))
	return String(conf.version)
}
