#!/usr/bin/env node
/**
 * Release a version: bump it, push, tag, publish the GitHub release, and start
 * the release workflow, which builds every platform into that release.
 *
 *   pnpm release 1.0.9                   # notes drafted from the commits since the last tag
 *   pnpm release 1.0.9 --notes notes.md  # your own notes (Markdown)
 *   pnpm release 1.0.9 --dry-run         # checks and notes only; changes nothing
 *
 * Everything is checked before anything changes. After a failure, run the same
 * command again: a bump or tag already pushed is reused.
 *
 * The release is published but not marked latest. The workflow marks it latest
 * once latest.json is uploaded, so installed apps never find a newest release
 * without update files.
 *
 * GitHub: a login that can push to the repo (GH_TOKEN, else `gh auth token -u yash1200`).
 */
import { execFileSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import process from 'node:process'
import { fileURLToPath, pathToFileURL } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const REPO = 'yash1200/mystique'
const GH_USER = 'yash1200'
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/

/** Each file holding the app version, and the one field in it: `before`, version, `after`. */
export const VERSION_FILES = [
	{ file: 'package.json', pattern: /^(\t"version": ")([^"]+)(",)$/m },
	{ file: 'src-tauri/Cargo.toml', pattern: /^(\[package\]\nname = "mystique"\nversion = ")([^"]+)(")$/m },
	{ file: 'src-tauri/tauri.conf.json', pattern: /^(\t"version": ")([^"]+)(",)$/m },
	{ file: 'Cargo.lock', pattern: /^(\[\[package\]\]\nname = "mystique"\nversion = ")([^"]+)(")$/m },
]

/** True when `next` is a higher x.y.z than `current` (pre-release suffixes are not ranked). */
export function isNewer(next, current) {
	const a = next.split(/[.-]/).slice(0, 3).map(Number)
	const b = current.split(/[.-]/).slice(0, 3).map(Number)
	for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] > b[i]
	return false
}

/** `text` with its version field set to `version`, and the version it had; null when the field is missing. */
export function setVersion(text, pattern, version) {
	const match = pattern.exec(text)
	if (!match) return null
	return {
		current: match[2],
		text: text.replace(pattern, (_, before, _old, after) => `${before}${version}${after}`),
	}
}

function fail(message) {
	console.error(`release: ${message}`)
	process.exit(1)
}

function step(message) {
	console.log(`\n\x1b[1m==> ${message}\x1b[0m`)
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
	const opts = { version: null, notes: null, dryRun: false }
	for (let i = 0; i < argv.length; i++) {
		const a = argv[i]
		if (a === '--notes') opts.notes = argv[++i] ?? fail('--notes needs a file')
		else if (a === '--dry-run') opts.dryRun = true
		else if (a === '-h' || a === '--help') {
			console.log('usage: pnpm release <version> [--notes notes.md] [--dry-run]')
			process.exit(0)
		} else if (a.startsWith('-')) fail(`unknown option ${a}`)
		else if (opts.version) fail('one version only')
		else opts.version = a.trim().replace(/^v/, '')
	}
	if (!opts.version) fail('usage: pnpm release <version>   e.g. pnpm release 1.0.9')
	if (!SEMVER.test(opts.version)) fail(`"${opts.version}" is not a version (x.y.z).`)
	return opts
}

/** Notes from the commits since the last tag, release bumps left out. */
function draftNotes() {
	const last = tryRun('git', ['describe', '--tags', '--abbrev=0', '--match', 'v*'])
	const log = run('git', ['log', '--no-merges', '--format=%s', last ? `${last}..HEAD` : 'HEAD'])
	const changes = log
		.split('\n')
		.filter((s) => s && !/^chore\(release\)/.test(s))
		.map((s) => `- ${s.replace(/^(feat|fix|perf|refactor|docs|chore|test|build|ci)(\([^)]*\))?: /, '')}`)
	return [
		'## Changes',
		'',
		changes.length ? changes.join('\n') : '- Maintenance release.',
		'',
		'## Install',
		'',
		'Download the file for your system below. Mystique 1.0.3 and later update themselves.',
		'',
		'macOS builds are not notarized: on first launch, right-click Mystique in Applications and choose **Open**.',
	].join('\n')
}

function main() {
	const opts = parseArgs(process.argv.slice(2))
	const { version } = opts
	const tag = `v${version}`

	// 1. Checks: nothing changes before these pass.
	step('Checking')
	if (run('git', ['rev-parse', '--abbrev-ref', 'HEAD']) !== 'main') fail('not on main.')
	if (run('git', ['status', '--porcelain', '--untracked-files=no'])) {
		fail('the working tree has changes. Commit or stash them first.')
	}
	run('git', ['fetch', '--quiet', 'origin', 'main', '--tags'])
	const head = run('git', ['rev-parse', 'HEAD'])
	if (head !== run('git', ['rev-parse', 'origin/main'])) fail('HEAD is not origin/main. Push or pull first.')

	const edits = VERSION_FILES.map(({ file, pattern }) => {
		const path = join(ROOT, file)
		const edit = setVersion(readFileSync(path, 'utf8'), pattern, version)
		if (!edit) fail(`no version field found in ${file}.`)
		return { file, path, ...edit }
	})
	const currents = new Set(edits.map((e) => e.current))
	if (currents.size > 1) {
		fail(`the version files disagree: ${edits.map((e) => `${e.file} ${e.current}`).join(', ')}.`)
	}
	const [current] = currents
	// Already at this version: a bump pushed by an earlier run that stopped later.
	const resume = current === version
	if (!resume && !isNewer(version, current)) fail(`${version} is not newer than the current ${current}.`)

	const tagged = tryRun('git', ['rev-parse', `${tag}^{commit}`])
	if (tagged && !(resume && tagged === head)) fail(`tag ${tag} already exists on another commit.`)

	if (!process.env.GH_TOKEN) {
		const token = tryRun('gh', ['auth', 'token', '-u', GH_USER])
		if (!token) fail(`gh has no login for ${GH_USER}. Run \`gh auth login\`, or set GH_TOKEN.`)
		process.env.GH_TOKEN = token
	}
	if (tryRun('gh', ['api', `repos/${REPO}`, '--jq', '.permissions.push']) !== 'true') {
		fail(`this GitHub login cannot push to ${REPO}.`)
	}
	if (tryRun('gh', ['release', 'view', tag, '--repo', REPO, '--json', 'tagName'])) {
		fail(`release ${tag} already exists. Run the release workflow for it.`)
	}
	console.log(`${current} → ${version}${resume ? ' (already bumped; resuming)' : ''}`)

	const notes = opts.notes ? readFileSync(resolve(opts.notes), 'utf8').trim() : draftNotes()
	const notesFile = join(ROOT, 'target', `release-notes-${tag}.md`)
	mkdirSync(dirname(notesFile), { recursive: true })
	writeFileSync(notesFile, `${notes}\n`)
	step(`Release notes (${notesFile})`)
	console.log(notes)

	if (opts.dryRun) {
		console.log('\nDry run: checks passed. Nothing was bumped, tagged or published.')
		return
	}

	// 2. Bump, commit, push.
	if (!resume) {
		step(`Bumping to ${version}`)
		for (const e of edits) {
			writeFileSync(e.path, e.text)
			console.log(`${e.file}: ${e.current} → ${version}`)
		}
		run('git', ['add', ...edits.map((e) => e.file)])
		run('git', ['commit', '-q', '-m', `chore(release): version ${version}`])
		run('git', ['push', '-q', 'origin', 'main'], { stdio: 'inherit' })
	}

	// 3. Tag and publish (not latest: the workflow flips that once the update files are up).
	step(`Tagging and publishing ${tag}`)
	if (!tagged) run('git', ['tag', '-a', tag, '-m', `Mystique ${version}`])
	run('git', ['push', '-q', 'origin', tag], { stdio: 'inherit' })
	run(
		'gh',
		[
			'release', 'create', tag,
			'--repo', REPO,
			'--verify-tag',
			'--title', `Release ${version}`,
			'--notes-file', notesFile,
			'--latest=false',
		],
		{ stdio: 'inherit' },
	)

	// 4. Build every platform into it.
	step(`Starting the release workflow for ${tag}`)
	const actions = `https://github.com/${REPO}/actions/workflows/release.yml`
	let started = true
	try {
		run('gh', ['workflow', 'run', 'release.yml', '--repo', REPO, '--ref', 'main', '-f', `tag=${tag}`], {
			stdio: 'inherit',
		})
	} catch {
		// The release is already published: say how to start the build by hand.
		started = false
	}

	step('Done')
	console.log(`https://github.com/${REPO}/releases/tag/${tag}`)
	if (started) console.log(`Builds: ${actions}`)
	else {
		console.log(`Could not start the workflow. Actions › release › Run workflow with ${tag}:`)
		console.log(`  ${actions}`)
	}
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) main()
