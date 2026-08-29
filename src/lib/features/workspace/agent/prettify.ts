/**
 * Fence language classification for chat code blocks.
 */
import type { SyntaxLang } from '$core/util'
import { inferFenceLang } from './markdown'

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
