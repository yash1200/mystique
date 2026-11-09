/** sql-formatter wrapper shared by every SQL engine adapter. */
import type { SqlLanguage } from 'sql-formatter'
import type { QueryFormatter } from '../types'

export function sqlFormatter(dialect: SqlLanguage): QueryFormatter {
	return async (text) => {
		const src = text.trim()
		if (!src) {
			return { ok: false, error: 'Nothing to format.', text }
		}
		try {
			// sql-formatter is the largest dependency; it loads on the first Format.
			const { format: formatSql } = await import('sql-formatter')
			const formatted = formatSql(src, {
				language: dialect,
				tabWidth: 2,
				keywordCase: 'upper',
				dataTypeCase: 'upper',
				functionCase: 'upper',
				logicalOperatorNewline: 'before',
				expressionWidth: 80,
				linesBetweenQueries: 1,
			})
			// Preserve a single trailing newline for editor comfort.
			const out = formatted.replace(/\s+$/, '') + '\n'
			return { ok: true, text: out }
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e)
			return { ok: false, error: msg || 'Could not format SQL.', text }
		}
	}
}
