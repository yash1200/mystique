/**
 * Chat-oriented markdown → AST.
 * Renders via Svelte (no raw HTML). CommonMark + GFM subset models emit:
 * ATX/setext headings, HR, fences, indented code, paragraphs, lists,
 * task lists, quotes, GitHub alerts, tables (with alignment), emphasis,
 * strikethrough, inline code, links, autolinks, images, $math$ / $$math$$.
 */
import type {
	MdAlertKind,
	MdAlign,
	MdBlock,
	MdInline,
	MdListItem,
} from './markdown-types'
export type { MdBlock, MdInline, MdListItem }

const ATX = /^ {0,3}(#{1,6})(?:[ \t]+(.*?))?[ \t]*#*[ \t]*$/
const SETEXT = /^ {0,3}(?:=+|-+)[ \t]*$/
const HR = /^ {0,3}(?:(?:-[\t ]*){3,}|(?:\*[ \t]*){3,}|(?:_[ \t]*){3,})$/
const UL = /^([ \t]*)([-*+])[ \t]+(.*)$/
const OL = /^([ \t]*)(\d{1,9})[.)][ \t]+(.*)$/
const TASK = /^\[([ xX])\][ \t]+(.*)$/
const BQ = /^[ \t]*>[ \t]?(.*)$/
const ALERT = /^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\][ \t]*$/i
const INDENT_CODE = /^(?:    |\t)/
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
	let steps = 0
	const stepLimit = lines.length * 8 + 64

	while (i < lines.length) {
		if (++steps > stepLimit) break
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

		const displayMath = readDisplayMath(lines, i)
		if (displayMath) {
			blocks.push({ type: 'math', value: displayMath.value })
			i = displayMath.next
			continue
		}

		const heading = line.match(ATX)
		if (heading) {
			const level = heading[1].length as 1 | 2 | 3 | 4 | 5 | 6
			blocks.push({
				type: 'h',
				level,
				children: parseInline(heading[2] ?? ''),
			})
			i += 1
			continue
		}

		if (
			line.trim() &&
			i + 1 < lines.length &&
			SETEXT.test(lines[i + 1]) &&
			!matchListMarker(line) &&
			!BQ.test(line) &&
			!isTableStart(lines, i)
		) {
			const level = lines[i + 1].trim().startsWith('=') ? 1 : 2
			blocks.push({
				type: 'h',
				level,
				children: parseInline(line.trim()),
			})
			i += 2
			continue
		}

		if (HR.test(line)) {
			blocks.push({ type: 'hr' })
			i += 1
			continue
		}

		const indented = readIndentedCode(lines, i)
		if (indented) {
			blocks.push({ type: 'code', lang: '', value: indented.value })
			i = indented.next
			continue
		}

		if (BQ.test(line)) {
			const quoted: string[] = []
			while (i < lines.length && BQ.test(lines[i])) {
				quoted.push(lines[i].replace(/^[ \t]*>[ \t]?/, ''))
				i += 1
			}
			const alert = quoted[0]?.trim().match(ALERT)
			if (alert) {
				const kind = alert[1].toLowerCase() as MdAlertKind
				blocks.push({
					type: 'alert',
					kind,
					children: parseBlocks(quoted.slice(1).join('\n')),
				})
			} else {
				blocks.push({
					type: 'blockquote',
					children: parseBlocks(quoted.join('\n')),
				})
			}
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
			if (next.trim().startsWith('$$')) break
			if (HR.test(next) || ATX.test(next) || SETEXT.test(next) || BQ.test(next))
				break
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

function readDisplayMath(
	lines: string[],
	start: number,
): { value: string; next: number } | null {
	const trimmed = lines[start].trim()
	if (!trimmed.startsWith('$$')) return null
	const after = trimmed.slice(2)
	const same = after.indexOf('$$')
	if (same >= 0) {
		return { value: after.slice(0, same).trim(), next: start + 1 }
	}
	const body: string[] = after ? [after] : []
	for (let i = start + 1; i < lines.length; i++) {
		const idx = lines[i].indexOf('$$')
		if (idx >= 0) {
			if (idx > 0) body.push(lines[i].slice(0, idx))
			return { value: body.join('\n').trim(), next: i + 1 }
		}
		body.push(lines[i])
	}
	return { value: body.join('\n').trim(), next: lines.length }
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
	const align = parseTableAlign(lines[start + 1], head.length)
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
			align,
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

function parseTableAlign(sep: string, cols: number): Array<MdAlign | null> {
	const cells = splitTableRow(sep)
	const out: Array<MdAlign | null> = []
	for (let c = 0; c < cols; c++) {
		const cell = (cells[c] ?? '').trim()
		const left = cell.startsWith(':')
		const right = cell.endsWith(':')
		out.push(left && right ? 'center' : right ? 'right' : left ? 'left' : null)
	}
	return out
}

function readIndentedCode(
	lines: string[],
	start: number,
): { value: string; next: number } | null {
	if (!INDENT_CODE.test(lines[start])) return null
	if (matchListMarker(lines[start])) return null
	const body: string[] = []
	let i = start
	while (i < lines.length) {
		const line = lines[i]
		if (!line.trim()) {
			body.push('')
			i += 1
			continue
		}
		if (!INDENT_CODE.test(line)) break
		if (matchListMarker(line) && listIndent(line) < 4) break
		body.push(line.replace(/^(?:    |\t)/, ''))
		i += 1
	}
	while (body.length && body[body.length - 1] === '') body.pop()
	if (!body.length) return null
	return { value: body.join('\n'), next: i }
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
	const items: { raw: string[]; checked?: boolean }[] = []
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
			const task = marker.rest.match(TASK)
			items.push(
				task
					? { raw: [task[2]], checked: task[1] !== ' ' }
					: { raw: [marker.rest] },
			)
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
		...(item.checked !== undefined ? { checked: item.checked } : {}),
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

function parseInline(src: string): MdInline[] {
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
		if (src.startsWith('$$', i)) {
			const math = findMath(src, i, '$$')
			if (math) {
				flush()
				out.push({ type: 'math', value: math.value })
				i = math.next
				continue
			}
		}
		if (ch === '$') {
			const math = findMath(src, i, '$')
			if (math) {
				flush()
				out.push({ type: 'math', value: math.value })
				i = math.next
				continue
			}
		}
		if (ch === '\\' && i + 1 < src.length && isAsciiPunct(src[i + 1]!)) {
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
		if (ch === '!' && src[i + 1] === '[') {
			const img = findImage(src, i)
			if (img) {
				flush()
				const srcUrl = sanitizeHref(img.href, 'image')
				if (srcUrl) {
					out.push({
						type: 'image',
						src: srcUrl,
						alt: img.alt,
						title: img.title,
					})
				} else {
					out.push({ type: 'text', value: img.alt || img.href })
				}
				i = img.next
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
					title: link.title,
					children: parseInline(link.label),
				})
				i = link.next
				continue
			}
		}
		if (ch === '<') {
			const auto = findAngleAutolink(src, i)
			if (auto?.href) {
				flush()
				out.push({
					type: 'link',
					href: auto.href,
					children: [{ type: 'text', value: auto.label }],
				})
				i = auto.next
				continue
			}
		}
		if (src.startsWith('http://', i) || src.startsWith('https://', i)) {
			const bare = findBareUrl(src, i)
			if (bare) {
				flush()
				out.push({
					type: 'link',
					href: bare.href,
					children: [{ type: 'text', value: bare.href }],
				})
				i = bare.next
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
		buf += ch
		i += 1
	}
	flush()
	return mergeText(out)
}

function isAsciiPunct(ch: string): boolean {
	return /[!"#$%&'()*+,\-./:;<=>?@[\\\]^_`{|}~]/.test(ch)
}

function findMath(
	src: string,
	start: number,
	delim: '$' | '$$',
): { value: string; next: number } | null {
	const n = delim.length
	if (src.slice(start, start + n) !== delim) return null
	if (delim === '$' && src[start + 1] === '$') return null
	const innerStart = start + n
	if (delim === '$' && (src[innerStart] === ' ' || src[innerStart] === '\n')) {
		return null
	}
	let i = innerStart
	while (i < src.length) {
		if (src[i] === '\\') {
			i += 2
			continue
		}
		if (delim === '$' && src[i] === '\n') return null
		if (src.slice(i, i + n) === delim) {
			const inner = src.slice(innerStart, i)
			if (!inner || inner.endsWith(' ') || inner.endsWith('\n')) return null
			return { value: inner, next: i + n }
		}
		i += 1
	}
	return null
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
): { label: string; href: string; title?: string; next: number } | null {
	if (src[start] !== '[') return null
	const dest = readLinkDestination(src, start + 1)
	if (!dest) return null
	if (!dest.label || !dest.href) return null
	return dest
}

function findImage(
	src: string,
	start: number,
): { alt: string; href: string; title?: string; next: number } | null {
	if (src[start] !== '!' || src[start + 1] !== '[') return null
	const dest = readLinkDestination(src, start + 2)
	if (!dest) return null
	return { alt: dest.label, href: dest.href, title: dest.title, next: dest.next }
}

function readLinkDestination(
	src: string,
	labelStart: number,
): { label: string; href: string; title?: string; next: number } | null {
	const closeLabel = src.indexOf(']', labelStart)
	if (closeLabel < 0 || src[closeLabel + 1] !== '(') return null
	let j = closeLabel + 2
	while (src[j] === ' ' || src[j] === '\t') j += 1
	let href = ''
	if (src[j] === '<') {
		const end = src.indexOf('>', j + 1)
		if (end < 0) return null
		href = src.slice(j + 1, end)
		j = end + 1
	} else {
		while (j < src.length && src[j] !== ' ' && src[j] !== '\t' && src[j] !== ')') {
			href += src[j]
			j += 1
		}
	}
	while (src[j] === ' ' || src[j] === '\t') j += 1
	let title: string | undefined
	if (src[j] === '"' || src[j] === "'") {
		const q = src[j]
		const end = src.indexOf(q, j + 1)
		if (end < 0) return null
		title = src.slice(j + 1, end)
		j = end + 1
		while (src[j] === ' ' || src[j] === '\t') j += 1
	}
	if (src[j] !== ')') return null
	return {
		label: src.slice(labelStart, closeLabel),
		href: href.trim(),
		title,
		next: j + 1,
	}
}

function findAngleAutolink(
	src: string,
	start: number,
): { href: string; label: string; next: number } | null {
	if (src[start] !== '<') return null
	const close = src.indexOf('>', start + 1)
	if (close <= start + 1) return null
	const inner = src.slice(start + 1, close).trim()
	if (/\s/.test(inner)) return null
	if (/^https?:\/\//i.test(inner) || /^mailto:/i.test(inner)) {
		return { href: sanitizeHref(inner), label: inner, next: close + 1 }
	}
	if (/^[^\s@/]+@[^\s@]+\.[^\s@]+$/.test(inner)) {
		return { href: `mailto:${inner}`, label: inner, next: close + 1 }
	}
	return null
}

function findBareUrl(
	src: string,
	start: number,
): { href: string; next: number } | null {
	if (start > 0 && /[A-Za-z0-9/_+-]/.test(src[start - 1] ?? '')) return null
	const slice = src.slice(start)
	const m = slice.match(/^https?:\/\/[^\s<]+/i)
	if (!m) return null
	let href = m[0]
	href = href.replace(/[),.;!?]+$/u, '')
	if (href.length < 11) return null
	return { href, next: start + href.length }
}

function sanitizeHref(href: string, kind: 'link' | 'image' = 'link'): string {
	const trimmed = href.trim()
	if (/^https?:\/\//i.test(trimmed)) return trimmed
	if (kind === 'link' && /^mailto:/i.test(trimmed)) return trimmed
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
