#!/usr/bin/env node
/**
 * Build and publish a macOS (Apple silicon) release with auto-update files.
 *
 *   pnpm bump 1.0.3 && git commit -am "chore(release): version 1.0.3" && git push
 *   pnpm release --notes notes.md
 *
 * Steps: check the tree is clean and pushed, build with the updater key
 * (`src-tauri/tauri.release.conf.json` turns the update archive on), write
 * `latest.json`, tag `v<version>`, and publish the GitHub release with the
 * .dmg, the update archive, its signature and `latest.json`. Installed apps
 * read `releases/latest/download/latest.json`, so publishing is the switch.
 *
 * Updater key (see docs/RELEASING.md):
 *   TAURI_SIGNING_PRIVATE_KEY           key text or path (default ~/.tauri/mystique.key)
 *   TAURI_SIGNING_PRIVATE_KEY_PASSWORD  else the keychain item "mystique-updater-key"
 * Apple signing, if set, is read by Tauri as in scripts/release-macos.sh.
 * GitHub: `gh` must be able to push to the repo (or set GH_TOKEN).
 *
 * Options:
 *   --notes <file>       release notes (Markdown) for the release and latest.json
 *   --skip-build         publish the artifacts already in the bundle folder
 *   --bundle-dir <dir>   where the bundle is (default target/aarch64-apple-darwin/release/bundle)
 *   --dry-run            build and write latest.json, but do not tag or publish
 */
import { execFileSync, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync, copyFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const REPO = 'yash1200/mystique'
const TARGET = 'aarch64-apple-darwin'
/** `latest.json` platform key for this build. Intel and Windows join later. */
const PLATFORM = 'darwin-aarch64'
const KEYCHAIN_ITEM = 'mystique-updater-key'

function fail(message) {
	console.error(`release: ${message}`)
	process.exit(1)
}

function run(cmd, args, opts = {}) {
	// `stdio: 'inherit'` returns null.
	return (execFileSync(cmd, args, { cwd: ROOT, encoding: 'utf8', ...opts }) ?? '').trim()
}

function tryRun(cmd, args) {
	try {
		return run(cmd, args, { stdio: ['ignore', 'pipe', 'ignore'] })
	} catch {
		return null
	}
}

function parseArgs(argv) {
	const opts = { notes: null, skipBuild: false, dryRun: false, bundleDir: null }
	for (let i = 0; i < argv.length; i++) {
		const a = argv[i]
		if (a === '--notes') opts.notes = argv[++i]
		else if (a === '--skip-build') opts.skipBuild = true
		else if (a === '--dry-run') opts.dryRun = true
		else if (a === '--bundle-dir') opts.bundleDir = argv[++i]
		else fail(`unknown option ${a}`)
	}
	return opts
}

const opts = parseArgs(process.argv.slice(2))
if (process.platform !== 'darwin') fail('macOS releases are built on a Mac.')

const version = JSON.parse(readFileSync(join(ROOT, 'src-tauri/tauri.conf.json'), 'utf8')).version
const tag = `v${version}`
const notes = opts.notes ? readFileSync(resolve(opts.notes), 'utf8').trim() : ''

// 1. The release must match what is on GitHub.
if (!opts.dryRun) {
	if (run('git', ['status', '--porcelain', '--untracked-files=no'])) {
		fail('the working tree has changes. Commit and push them first.')
	}
	run('git', ['fetch', '--quiet', 'origin', 'main', '--tags'])
	if (run('git', ['rev-parse', 'HEAD']) !== run('git', ['rev-parse', 'origin/main'])) {
		fail('HEAD is not origin/main. Push (or pull) first.')
	}
	const tagged = tryRun('git', ['rev-parse', `${tag}^{commit}`])
	if (tagged && tagged !== run('git', ['rev-parse', 'HEAD'])) {
		fail(`${tag} already points at another commit. Bump the version (pnpm bump).`)
	}
	if (tryRun('gh', ['release', 'view', tag, '--repo', REPO, '--json', 'tagName'])) {
		fail(`release ${tag} already exists on GitHub.`)
	}
	if (tryRun('gh', ['api', `repos/${REPO}`, '--jq', '.permissions.push']) !== 'true') {
		fail(
			`gh cannot push to ${REPO}. Run \`gh auth switch -u yash1200\`, or set ` +
				'GH_TOKEN=$(gh auth token -u yash1200).',
		)
	}
}

// 2. Build with the updater key.
const bundleDir = resolve(opts.bundleDir ?? join(ROOT, 'target', TARGET, 'release', 'bundle'))
if (!opts.skipBuild) {
	const env = { ...process.env }
	if (!env.TAURI_SIGNING_PRIVATE_KEY) {
		const keyPath = join(homedir(), '.tauri', 'mystique.key')
		if (!existsSync(keyPath)) fail(`no updater key: set TAURI_SIGNING_PRIVATE_KEY or create ${keyPath}.`)
		env.TAURI_SIGNING_PRIVATE_KEY = readFileSync(keyPath, 'utf8').trim()
	}
	if (env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD == null) {
		const fromKeychain = tryRun('security', ['find-generic-password', '-s', KEYCHAIN_ITEM, '-w'])
		if (fromKeychain == null) {
			fail(
				`no key password: set TAURI_SIGNING_PRIVATE_KEY_PASSWORD, or store it once with\n` +
					`  security add-generic-password -a "$USER" -s ${KEYCHAIN_ITEM} -w`,
			)
		}
		env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = fromKeychain
	}
	const build = spawnSync(
		'pnpm',
		[
			'tauri',
			'build',
			'--target',
			TARGET,
			'--bundles',
			'app,dmg',
			'--config',
			'src-tauri/tauri.release.conf.json',
		],
		{ cwd: ROOT, env, stdio: 'inherit' },
	)
	if (build.status !== 0) fail('the build failed.')
}

// 3. The files to publish.
const dmg = join(bundleDir, 'dmg', `Mystique_${version}_aarch64.dmg`)
const archive = join(bundleDir, 'macos', 'Mystique.app.tar.gz')
for (const file of [dmg, archive, `${archive}.sig`]) {
	if (!existsSync(file)) fail(`missing ${file}. Build first, or check --bundle-dir.`)
}
const outDir = join(bundleDir, 'release', tag)
mkdirSync(outDir, { recursive: true })
// One archive name per platform, so Intel and Windows can sit beside it later.
const archiveName = `Mystique_${version}_aarch64.app.tar.gz`
copyFileSync(archive, join(outDir, archiveName))
copyFileSync(`${archive}.sig`, join(outDir, `${archiveName}.sig`))
copyFileSync(dmg, join(outDir, `Mystique_${version}_aarch64.dmg`))

const latest = {
	version,
	notes: notes || `See https://github.com/${REPO}/releases/tag/${tag}`,
	pub_date: new Date().toISOString(),
	platforms: {
		[PLATFORM]: {
			signature: readFileSync(`${archive}.sig`, 'utf8').trim(),
			url: `https://github.com/${REPO}/releases/download/${tag}/${archiveName}`,
		},
	},
}
writeFileSync(join(outDir, 'latest.json'), `${JSON.stringify(latest, null, 2)}\n`)
const files = [`Mystique_${version}_aarch64.dmg`, archiveName, `${archiveName}.sig`, 'latest.json'].map(
	(f) => join(outDir, f),
)
console.log(`\nRelease files for ${tag} in ${outDir}:`)
for (const f of files) console.log(`  ${f.slice(outDir.length + 1)}`)

if (opts.dryRun) {
	console.log('\nDry run: nothing tagged or published.')
	process.exit(0)
}

// 4. Tag and publish.
if (!tryRun('git', ['rev-parse', `${tag}^{commit}`])) {
	run('git', ['tag', '-a', tag, '-m', `Mystique ${version}`])
}
run('git', ['push', 'origin', tag], { stdio: 'inherit' })
const notesFile = join(outDir, 'notes.md')
writeFileSync(notesFile, notes ? `${notes}\n` : `Mystique ${version}.\n`)
run(
	'gh',
	[
		'release',
		'create',
		tag,
		...files,
		'--repo',
		REPO,
		'--verify-tag',
		'--title',
		`Release ${version}`,
		'--notes-file',
		notesFile,
		'--latest',
	],
	{ stdio: 'inherit' },
)
console.log(`\nPublished ${tag}. Installed apps see it on their next update check.`)
