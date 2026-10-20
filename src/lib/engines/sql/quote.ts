/** Identifier quotes. Matches `quote_ident` in the connector API. */

export type IdentQuote = 'postgres' | 'mysql'

export function quoteForKind(kind: 'postgresql' | 'mysql' | 'sqlite'): IdentQuote {
	return kind === 'mysql' ? 'mysql' : 'postgres'
}

export function quoteIdent(style: IdentQuote, id: string): string {
	if (style === 'mysql') return `\`${id.replace(/`/g, '``')}\``
	return `"${id.replace(/"/g, '""')}"`
}
