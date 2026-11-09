/**
 * `@table` references in the composer. The text is the source of truth: at
 * send time every `@name` / `@schema.name` that matches a table in the
 * active connection's catalog becomes a reference, and the backend sends
 * that table's columns with the turn.
 */
import type { AgentTableRef } from '#core/contracts'

export type MentionTable = {
	database: string
	schema: string
	name: string
}

const TOKEN = /[A-Za-z0-9_$.]/

export function mentionLabel(table: Pick<MentionTable, 'schema' | 'name'>): string {
	return table.schema ? `${table.schema}.${table.name}` : table.name
}

/** The `@query` being typed at `caret`, if any. */
export function activeMention(
	text: string,
	caret: number,
): { start: number; query: string } | null {
	let i = caret
	while (i > 0 && TOKEN.test(text[i - 1] ?? '')) i -= 1
	if (i === 0 || text[i - 1] !== '@') return null
	const at = i - 1
	if (at > 0 && !/\s|\(/.test(text[at - 1] ?? '')) return null
	return { start: at, query: text.slice(i, caret) }
}

/** Tables for the popup: prefix matches first, then substring, at most `limit`. */
export function mentionCandidates(
	tables: MentionTable[],
	query: string,
	limit = 8,
): MentionTable[] {
	const q = query.toLowerCase()
	const scored: { table: MentionTable; score: number }[] = []
	for (const table of tables) {
		const name = table.name.toLowerCase()
		const label = mentionLabel(table).toLowerCase()
		let score = -1
		if (!q) score = 2
		else if (name.startsWith(q) || label.startsWith(q)) score = 0
		else if (label.includes(q)) score = 1
		if (score >= 0) scored.push({ table, score })
	}
	scored.sort(
		(a, b) => a.score - b.score || mentionLabel(a.table).localeCompare(mentionLabel(b.table)),
	)
	return scored.slice(0, limit).map((s) => s.table)
}

/** Replace the `@query` at `start…caret` with `@label `. */
export function insertMention(
	text: string,
	start: number,
	caret: number,
	table: Pick<MentionTable, 'schema' | 'name'>,
): { text: string; caret: number } {
	const token = `@${mentionLabel(table)} `
	const next = text.slice(0, start) + token + text.slice(caret).replace(/^ /, '')
	return { text: next, caret: start + token.length }
}

/** Append a reference (drag and drop), with a space before it when needed. */
export function appendMention(
	text: string,
	table: Pick<MentionTable, 'schema' | 'name'>,
): string {
	const token = `@${mentionLabel(table)} `
	if (!text) return token
	return /\s$/.test(text) ? text + token : `${text} ${token}`
}

/** Every `@reference` in `text` that names a known table, once each. */
export function resolveMentions(text: string, tables: MentionTable[]): AgentTableRef[] {
	const out: AgentTableRef[] = []
	const seen = new Set<string>()
	for (const match of text.matchAll(/(^|[\s(])@([A-Za-z0-9_$.]+)/g)) {
		const raw = (match[2] ?? '').replace(/\.+$/, '').toLowerCase()
		if (!raw) continue
		const table =
			tables.find((t) => mentionLabel(t).toLowerCase() === raw) ??
			tables.find((t) => t.name.toLowerCase() === raw)
		if (!table) continue
		const key = mentionLabel(table)
		if (seen.has(key)) continue
		seen.add(key)
		out.push({ schema: table.schema || null, name: table.name })
	}
	return out
}
