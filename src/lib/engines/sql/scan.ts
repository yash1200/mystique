/** Byte-level helpers shared by the SQL scanners (split, scope, params). */

/** ASCII `[A-Za-z_]`. */
export function isIdentStart(code: number): boolean {
	return (code >= 65 && code <= 90) || (code >= 97 && code <= 122) || code === 95
}

/** ASCII `[A-Za-z0-9_]`. */
export function isIdentByte(code: number): boolean {
	return isIdentStart(code) || isDigit(code)
}

/** ASCII `[0-9]`. */
export function isDigit(code: number): boolean {
	return code >= 48 && code <= 57
}

/** Index just past `tag` at or after `index`; the end of `sql` when unclosed. */
export function findCloser(sql: string, index: number, tag: string): number {
	const at = sql.indexOf(tag, index)
	return at < 0 ? sql.length : at + tag.length
}
