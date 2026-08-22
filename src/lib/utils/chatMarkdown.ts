/**
 * Chat-oriented markdown → AST.
 * Renders via Svelte (no HTML). Supports the subset models actually emit:
 * headings, paragraphs, emphasis, inline code, fences, lists, quotes, tables, hr, links.
 */

export type MdInline =
	| { type: 'text'; value: string }
	| { type: 'code'; value: string }
	| { type: 'strong'; children: MdInline[] }
	| { type: 'em'; children: MdInline[] }
	| { type: 'del'; children: MdInline[] }
	| { type: 'link'; href: string; children: MdInline[] }
	| { type: 'break' }

export type MdListItem = { children: MdBlock[] }

export type MdBlock =
	| { type: 'p'; children: MdInline[] }
	| { type: 'h'; level: 1 | 2 | 3; children: MdInline[] }
	| { type: 'code'; lang: string; value: string }
	| { type: 'ul'; items: MdListItem[] }
	| { type: 'ol'; start: number; items: MdListItem[] }
	| { type: 'blockquote'; children: MdBlock[] }
	| { type: 'table'; head: MdInline[][]; rows: MdInline[][][] }
	| { type: 'hr' }

const ATX = /^(#{1,3})[ \t]+(.+?)[ \t]*#*[ \t]*$/
const HR = /^(?:-{3,}|\*{3,}|_{3,})[ \t]*$/
const UL = /^([ \t]*)([-*+])[ \t]+(.*)$/
const OL = /^([ \t]*)(\d{1,9})[.)][ \t]+(.*)$/
const BQ = /^[ \t]*>[ \t]?(.*)$/
const TABLE_ROW = /^\s*\|?[^|\n]+(?:\|[^|\n]*)+\|?\s*$/
const TABLE_SEP = /^\s*\|?(?:\s*:?-{2,}:?\s*\|)+\s*:?-{2,}:?\s*\|?\s*$/

export function parseChatMarkdown(src: string): MdBlock[] {
	const text = src.replace(/\r\n/g, '\n').replace(/\r/g, '\n')
	if (!text.trim()) return []
	return parseBlocks(text)
}

function parseBlocks(src: string): MdBlock[] {
	const lines = src.split('\n')
	const blocks: MdBlock[] = []
	let i = 0

	while (i < lines.length) {
		const line = lines[i]
		if (!line.trim()) {
			i += 1
			continue
		}

		const fence = readFence(lines, i)
		if (fence) {
			blocks.push({ type: 'code', lang: fence.lang, value: fence.value })
			i = fence.next
			continue
		}

		if (HR.test(line.trim())) {
			blocks.push({ type: 'hr' })
			i += 1
			continue
		}

		const heading = line.match(ATX)
		if (heading) {
			const level = heading[1].length as 1 | 2 | 3
			blocks.push({ type: 'h', level, children: parseInline(heading[2]) })
			i += 1
			continue
		}

		if (BQ.test(line)) {
			const quoted: string[] = []
			while (i < lines.length && BQ.test(lines[i])) {
				quoted.push(lines[i].replace(/^[ \t]*>[ \t]?/, ''))
				i += 1
			}
			blocks.push({
				type: 'blockquote',
				children: parseBlocks(quoted.join('\n')),
			})
			continue
		}

		if (isTableStart(lines, i)) {
			const table = readTable(lines, i)
			blocks.push(table.block)
			i = table.next
			continue
		}

		if (UL.test(line) || OL.test(line)) {
			const list = readList(lines, i)
			blocks.push(list.block)
			i = list.next
			continue
		}

		const para: string[] = [line]
		i += 1
		while (i < lines.length) {
			const next = lines[i]
			if (!next.trim()) break
			if (readFence(lines, i)) break
			if (HR.test(next.trim()) || ATX.test(next) || BQ.test(next)) break
			if (UL.test(next) || OL.test(next)) break
			if (isTableStart(lines, i)) break
			para.push(next)
			i += 1
		}
		blocks.push({ type: 'p', children: parseInline(para.join('\n')) })
	}

	return blocks
}

function readFence(
	lines: string[],
	start: number,
): { lang: string; value: string; next: number } | null {
	const open = lines[start].match(/^([ \t]*)(`{3,}|~{3,})([^\n]*)$/)
	if (!open) return null
	const marker = open[2]
	const lang = open[3].trim().split(/\s+/)[0] ?? ''
	const body: string[] = []
	for (let i = start + 1; i < lines.length; i++) {
		if (lines[i].trim() === marker) {
			return { lang, value: body.join('\n'), next: i + 1 }
		}
		body.push(lines[i])
	}
	// Unclosed fence: consume the rest so we still render as code.
	return { lang, value: body.join('\n'), next: lines.length }
}

function isTableStart(lines: string[], i: number): boolean {
	return (
		TABLE_ROW.test(lines[i]) &&
		i + 1 < lines.length &&
		TABLE_SEP.test(lines[i + 1])
	)
}

function readTable(
	lines: string[],
	start: number,
): { block: Extract<MdBlock, { type: 'table' }>; next: number } {
	const head = splitTableRow(lines[start])
	let i = start + 2
	const rows: string[][] = []
	while (i < lines.length && TABLE_ROW.test(lines[i])) {
		rows.push(splitTableRow(lines[i]))
		i += 1
	}
	const cols = head.length
	return {
		block: {
			type: 'table',
			head: head.map(parseInline),
			rows: rows.map((row) => {
				const cells = row.slice(0, cols)
				while (cells.length < cols) cells.push('')
				return cells.map(parseInline)
			}),
		},
		next: i,
	}
}

function splitTableRow(line: string): string[] {
	let body = line.trim()
	if (body.startsWith('|')) body = body.slice(1)
	if (body.endsWith('|')) body = body.slice(0, -1)
	return body.split('|').map((c) => c.trim())
}

function readList(
	lines: string[],
	start: number,
): { block: Extract<MdBlock, { type: 'ul' | 'ol' }>; next: number } {
	const first = lines[start]
	const ol = first.match(OL)
	const kind: 'ul' | 'ol' = ol ? 'ol' : 'ul'
	const startN = ol ? Number(ol[2]) : 1
	const baseIndent = listIndent(first)
	const items: { raw: string[] }[] = []
	let i = start

	while (i < lines.length) {
		const line = lines[i]
		if (!line.trim()) {
			if (i + 1 < lines.length && continuesList(lines[i + 1], baseIndent)) {
				items[items.length - 1]?.raw.push('')
				i += 1
				continue
			}
			break
		}
		const marker = matchListMarker(line)
		if (
			marker &&
			marker.indent <= baseIndent + 1 &&
			marker.indent >= baseIndent - 1
		) {
			items.push({ raw: [marker.rest] })
			i += 1
			continue
		}
		if (marker && marker.indent > baseIndent + 1) {
			items[items.length - 1]?.raw.push(line)
			i += 1
			continue
		}
		if (/^[ \t]/.test(line) && items.length) {
			items[items.length - 1].raw.push(line.replace(/^[ \t]{1,4}/, ''))
			i += 1
			continue
		}
		break
	}

	const parsed: MdListItem[] = items.map((item) => ({
		children: parseBlocks(item.raw.join('\n')),
	}))

	if (kind === 'ol') {
		return { block: { type: 'ol', start: startN, items: parsed }, next: i }
	}
	return { block: { type: 'ul', items: parsed }, next: i }
}

function listIndent(line: string): number {
	const m = line.match(/^([ \t]*)/)
	return expandTabs(m?.[1] ?? '').length
}

function matchListMarker(
	line: string,
): { indent: number; rest: string } | null {
	const ul = line.match(UL)
	if (ul) return { indent: expandTabs(ul[1]).length, rest: ul[3] }
	const ol = line.match(OL)
	if (ol) return { indent: expandTabs(ol[1]).length, rest: ol[3] }
	return null
}

function continuesList(line: string, baseIndent: number): boolean {
	const marker = matchListMarker(line)
	if (marker) return marker.indent >= baseIndent
	return /^[ \t]/.test(line)
}

function expandTabs(s: string): string {
	return s.replace(/\t/g, '    ')
}

export function parseInline(src: string): MdInline[] {
	const out: MdInline[] = []
	let i = 0
	let buf = ''

	const flush = () => {
		if (buf) {
			out.push({ type: 'text', value: buf })
			buf = ''
		}
	}

	while (i < src.length) {
		const ch = src[i]
		if (ch === '\\' && i + 1 < src.length) {
			buf += src[i + 1]
			i += 2
			continue
		}
		if (ch === '\n') {
			flush()
			out.push({ type: 'break' })
			i += 1
			continue
		}
		if (ch === '`') {
			const end = findInlineCode(src, i)
			if (end) {
				flush()
				out.push({ type: 'code', value: end.value })
				i = end.next
				continue
			}
		}
		if (src.startsWith('**', i) || src.startsWith('__', i)) {
			const delim = src.slice(i, i + 2)
			const end = findDelim(src, i, delim)
			if (end) {
				flush()
				out.push({ type: 'strong', children: parseInline(end.inner) })
				i = end.next
				continue
			}
		}
		if (src.startsWith('~~', i)) {
			const end = findDelim(src, i, '~~')
			if (end) {
				flush()
				out.push({ type: 'del', children: parseInline(end.inner) })
				i = end.next
				continue
			}
		}
		if ((ch === '*' || ch === '_') && looksLikeEmOpen(src, i, ch)) {
			const end = findEm(src, i, ch)
			if (end) {
				flush()
				out.push({ type: 'em', children: parseInline(end.inner) })
				i = end.next
				continue
			}
		}
		if (ch === '[') {
			const link = findLink(src, i)
			if (link) {
				flush()
				out.push({
					type: 'link',
					href: sanitizeHref(link.href),
					children: parseInline(link.label),
				})
				i = link.next
				continue
			}
		}
		buf += ch
		i += 1
	}
	flush()
	return mergeText(out)
}

function findInlineCode(
	src: string,
	start: number,
): { value: string; next: number } | null {
	let ticks = 0
	while (src[start + ticks] === '`') ticks += 1
	const open = src.slice(start, start + ticks)
	const close = src.indexOf(open, start + ticks)
	if (close < 0) return null
	return {
		value: src.slice(start + ticks, close).replace(/\n/g, ' '),
		next: close + ticks,
	}
}

function findDelim(
	src: string,
	start: number,
	delim: string,
): { inner: string; next: number } | null {
	const from = start + delim.length
	const close = src.indexOf(delim, from)
	if (close <= from) return null
	if (src[close - 1] === '\\') return null
	return { inner: src.slice(from, close), next: close + delim.length }
}

function looksLikeEmOpen(src: string, i: number, mark: string): boolean {
	const next = src[i + 1]
	if (!next || next === ' ' || next === '\n' || next === mark) return false
	if (mark === '_' && i > 0 && /[A-Za-z0-9]/.test(src[i - 1])) return false
	return true
}

function findEm(
	src: string,
	start: number,
	mark: string,
): { inner: string; next: number } | null {
	for (let i = start + 1; i < src.length; i++) {
		if (src[i] === '\\') {
			i += 1
			continue
		}
		if (src[i] !== mark) continue
		if (i === start + 1) return null
		if (src[i - 1] === ' ' || src[i - 1] === '\n') continue
		if (mark === '_' && /[A-Za-z0-9]/.test(src[i + 1] ?? '')) continue
		return { inner: src.slice(start + 1, i), next: i + 1 }
	}
	return null
}

function findLink(
	src: string,
	start: number,
): { label: string; href: string; next: number } | null {
	if (src[start] !== '[') return null
	const closeLabel = src.indexOf(']', start + 1)
	if (closeLabel < 0 || src[closeLabel + 1] !== '(') return null
	const closeHref = src.indexOf(')', closeLabel + 2)
	if (closeHref < 0) return null
	const label = src.slice(start + 1, closeLabel)
	const href = src.slice(closeLabel + 2, closeHref).trim()
	if (!label || !href) return null
	return { label, href, next: closeHref + 1 }
}

function sanitizeHref(href: string): string {
	const trimmed = href.trim()
	if (/^https?:\/\//i.test(trimmed)) return trimmed
	if (/^mailto:/i.test(trimmed)) return trimmed
	return ''
}

function mergeText(nodes: MdInline[]): MdInline[] {
	const out: MdInline[] = []
	for (const n of nodes) {
		const last = out[out.length - 1]
		if (n.type === 'text' && last?.type === 'text') {
			last.value += n.value
		} else {
			out.push(n)
		}
	}
	return out
}

/** True when a fence should offer Apply-to-editor. */
export function isQueryLang(lang: string): boolean {
	const l = lang.trim().toLowerCase()
	return (
		l === 'sql' ||
		l === 'postgresql' ||
		l === 'postgres' ||
		l === 'mysql' ||
		l === 'mariadb' ||
		l === 'sqlite' ||
		l === 'plsql' ||
		l === 'tsql' ||
		l === 'mql' ||
		l === 'mongodb' ||
		l === 'mongo' ||
		l === 'javascript' ||
		l === 'js'
	)
}

/** Guess a fence language when the model omitted one. */
export function inferFenceLang(lang: string, code: string): string {
	const given = lang.trim().toLowerCase()
	if (given) return given
	const t = code.trim()
	if (!t) return ''
	if (
		/^(select|with|insert|update|delete|create|alter|drop|explain|comment|truncate|grant|revoke)\b/i.test(
			t,
		)
	) {
		return 'sql'
	}
	if (/^db\.[A-Za-z_][\w$]*\./.test(t)) return 'mql'
	if (t.startsWith('{') || t.startsWith('[')) return 'json'
	return ''
}
