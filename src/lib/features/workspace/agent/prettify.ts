/**
 * Fence language classification and pretty-printing for chat code blocks.
 * Lives here, not in `$core/util`, because it asks `$engines` how to format.
 */
import type { SyntaxLang } from '$core/util'
import { formatQuery } from '$engines'
import { inferFenceLang } from './markdown'

const PRETTY_MAX = 8_000

export function classifyLang(lang: string, source: string): SyntaxLang {
	const l = inferFenceLang(lang, source)
	if (
		l === 'sql' ||
		l === 'postgresql' ||
		l === 'postgres' ||
		l === 'mysql' ||
		l === 'mariadb' ||
		l === 'sqlite' ||
		l === 'plsql' ||
		l === 'tsql'
	) {
		return 'sql'
	}
	if (l === 'json') return 'json'
	if (
		l === 'javascript' ||
		l === 'js' ||
		l === 'ts' ||
		l === 'typescript' ||
		l === 'mql' ||
		l === 'mongodb' ||
		l === 'mongo'
	) {
		return 'js'
	}
	return 'plain'
}

/** Pretty-print SQL / MQL / JSON. Falls back to the original on failure. */
export function prettifyCode(
	lang: string,
	source: string,
	engine: string,
): string {
	const raw = source.replace(/\s+$/, '')
	if (!raw.trim()) return source
	if (raw.length > PRETTY_MAX) {
		return source.endsWith('\n') ? source : `${source}\n`
	}
	const kind = classifyLang(lang, raw)
	if (kind === 'json') {
		try {
			return JSON.stringify(JSON.parse(raw), null, 2) + '\n'
		} catch {
			return source.endsWith('\n') ? source : source + '\n'
		}
	}
	if (kind === 'sql') {
		const result = formatQuery(engine || 'sql', raw)
		return result.ok ? result.text : raw + '\n'
	}
	if (kind === 'js') {
		const id = /mongo|mql/.test(lang.toLowerCase())
			? 'mongodb'
			: engine.toLowerCase() === 'mongodb' || engine.toLowerCase() === 'mongo'
				? 'mongodb'
				: ''
		if (id) {
			const result = formatQuery(id, raw)
			if (result.ok) return result.text
		}
		return raw + '\n'
	}
	return source.endsWith('\n') ? source : source + '\n'
}
