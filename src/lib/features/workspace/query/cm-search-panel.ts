/**
 * Floating find/replace widget — VS Code layout, Mystique chrome.
 * Mounted on the editor root so it sits top-right without taking a strip.
 */
import {
	SearchQuery,
	closeSearchPanel,
	findNext,
	findPrevious,
	getSearchQuery,
	openSearchPanel,
	replaceAll,
	replaceNext,
	search,
	searchPanelOpen,
	setSearchQuery,
} from '@codemirror/search'
import { Prec, type EditorState, type Extension } from '@codemirror/state'
import {
	EditorView,
	ViewPlugin,
	keymap,
	runScopeHandlers,
	type ViewUpdate,
} from '@codemirror/view'
import { iconInner, type IconName } from '$icons'

const MATCH_CAP = 2_000

const ICON_REPLACE =
	'<path d="M3.5 4.25h6.25M9.75 4.25 7.75 2.5M9.75 4.25 7.75 6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/><path d="M12.5 11.75H6.25M6.25 11.75 8.25 13.5M6.25 11.75 8.25 10" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>'
const ICON_REPLACE_ALL =
	'<path d="M3.25 3.5h7M10.25 3.5 8.25 1.85M10.25 3.5 8.25 5.15" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/><path d="M3.25 8h7M10.25 8 8.25 6.35M10.25 8 8.25 9.65" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/><path d="M3.25 12.5h7M10.25 12.5 8.25 10.85M10.25 12.5 8.25 14.15" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>'
const ICON_IN_SELECTION =
	'<path d="M3.25 4.25h9.5M3.25 8h6.25M3.25 11.75h9.5" stroke="currentColor" stroke-width="1.35" stroke-linecap="round"/>'

/** Search state + floating widget + ⌘F / Escape. */
export function mystiqueSearch(): Extension {
	return [
		search({
			top: true,
			createPanel: () => {
				const dom = document.createElement('div')
				dom.className = 'cm-mystique-search-slot'
				return { dom, top: true }
			},
		}),
		searchOverlay,
		Prec.highest(
			keymap.of([
				{
					key: 'Mod-f',
					run: (view) => {
						openSearchPanel(view)
						focusFindField(view)
						return true
					},
				},
				{
					key: 'Escape',
					run: (view) => {
						if (!searchPanelOpen(view.state)) return false
						closeSearchPanel(view)
						view.focus()
						return true
					},
				},
			]),
		),
		EditorView.scrollMargins.of((view) => {
			if (!searchPanelOpen(view.state)) return null
			return { right: 456, top: 8 }
		}),
	]
}

function focusFindField(view: EditorView) {
	const input = view.dom.querySelector<HTMLInputElement>(
		'.cm-mystique-search [main-field]',
	)
	if (!input) return
	input.focus()
	input.select()
}

const searchOverlay = ViewPlugin.fromClass(
	class {
		private readonly ui: MystiqueSearchPanel

		constructor(readonly view: EditorView) {
			this.ui = new MystiqueSearchPanel(view)
			this.sync()
		}

		update(update: ViewUpdate) {
			this.sync()
			if (searchPanelOpen(update.state)) this.ui.update(update)
		}

		destroy() {
			this.ui.dom.remove()
		}

		private sync() {
			const open = searchPanelOpen(this.view.state)
			if (open && !this.ui.dom.isConnected) {
				this.view.dom.appendChild(this.ui.dom)
				this.ui.focusFind()
			} else if (!open && this.ui.dom.isConnected) {
				this.ui.dom.remove()
			}
		}
	},
)

class MystiqueSearchPanel {
	readonly dom: HTMLElement

	private readonly view: EditorView
	private query: SearchQuery
	private replaceOpen = false
	private preserveCase = false
	private inSelection = false
	private readonly findField: HTMLInputElement
	private readonly replaceField: HTMLInputElement
	private readonly replaceFieldWrap: HTMLElement
	private readonly replaceActions: HTMLElement
	private readonly countEl: HTMLElement
	private readonly caseBtn: HTMLButtonElement
	private readonly reBtn: HTMLButtonElement
	private readonly wordBtn: HTMLButtonElement
	private readonly preserveBtn: HTMLButtonElement
	private readonly inSelBtn: HTMLButtonElement
	private readonly replaceToggle: HTMLButtonElement

	constructor(view: EditorView) {
		this.view = view
		this.query = getSearchQuery(view.state)
		this.commit = this.commit.bind(this)

		this.findField = field({
			value: this.query.search,
			placeholder: 'Find',
			label: 'Find',
			main: true,
			oninput: this.commit,
		})
		this.replaceField = field({
			value: this.query.replace,
			placeholder: 'Replace',
			label: 'Replace',
			oninput: this.commit,
		})
		this.countEl = el('span', {
			class: 'cm-mystique-search-count',
			'aria-live': 'polite',
		})
		this.caseBtn = toggleBtn('Aa', 'Match case', this.query.caseSensitive, this.commit)
		this.wordBtn = toggleBtn(
			'ab',
			'Whole word',
			this.query.wholeWord,
			this.commit,
			'cm-mystique-search-word',
		)
		this.reBtn = toggleBtn('.*', 'Regular expression', this.query.regexp, this.commit)
		this.preserveBtn = toggleBtn('AB', 'Preserve case', false, () => {
			this.preserveCase = this.preserveBtn.getAttribute('aria-pressed') === 'true'
		})
		this.inSelBtn = markBtn(
			ICON_IN_SELECTION,
			'Find in selection',
			false,
			() => {
				this.inSelection = this.inSelBtn.getAttribute('aria-pressed') === 'true'
				this.commit()
			},
		)
		this.replaceToggle = iconBtn('chevron-right', 'Show replace', () =>
			this.toggleReplace(),
		)
		this.replaceToggle.classList.add('cm-mystique-search-twist')
		this.replaceToggle.setAttribute('aria-expanded', 'false')
		if (view.state.readOnly) this.replaceToggle.hidden = true

		this.replaceFieldWrap = el(
			'div',
			{ class: 'cm-mystique-search-fieldwrap', hidden: true },
			[
				this.replaceField,
				el('div', { class: 'cm-mystique-search-infield' }, [this.preserveBtn]),
			],
		)
		this.replaceActions = el(
			'div',
			{ class: 'cm-mystique-search-actions', hidden: true },
			[
				markBtn(ICON_REPLACE, 'Replace', false, () => this.replaceOne(), false),
				markBtn(
					ICON_REPLACE_ALL,
					'Replace all',
					false,
					() => this.replaceEvery(),
					false,
				),
			],
		)

		this.dom = el(
			'div',
			{
				class: 'cm-search cm-mystique-search cm-mystique-search-collapsed',
				role: 'search',
			},
			[
				this.replaceToggle,
				el('div', { class: 'cm-mystique-search-fieldwrap' }, [
					this.findField,
					el('div', { class: 'cm-mystique-search-infield' }, [
						this.caseBtn,
						this.wordBtn,
						this.reBtn,
					]),
				]),
				el('div', { class: 'cm-mystique-search-meta' }, [
					this.countEl,
					iconBtn('arrow-up', 'Previous match', () => findPrevious(this.view)),
					iconBtn('arrow-down', 'Next match', () => findNext(this.view)),
					this.inSelBtn,
					iconBtn('close', 'Close', () => closeSearchPanel(this.view)),
				]),
				this.replaceFieldWrap,
				this.replaceActions,
			],
		)
		this.dom.addEventListener('keydown', (e) => this.keydown(e))
		this.paint()
	}

	focusFind() {
		this.findField.focus()
		this.findField.select()
	}

	update(_update: ViewUpdate) {
		this.paint()
	}

	private toggleReplace() {
		this.replaceOpen = !this.replaceOpen
		this.dom.classList.toggle('cm-mystique-search-collapsed', !this.replaceOpen)
		this.replaceFieldWrap.hidden = !this.replaceOpen
		this.replaceActions.hidden = !this.replaceOpen
		this.replaceToggle.setAttribute('aria-expanded', String(this.replaceOpen))
		this.replaceToggle.title = this.replaceOpen ? 'Hide replace' : 'Show replace'
		this.replaceToggle.setAttribute(
			'aria-label',
			this.replaceOpen ? 'Hide replace' : 'Show replace',
		)
		this.replaceToggle.replaceChildren(
			icon(this.replaceOpen ? 'chevron-down' : 'chevron-right'),
		)
		if (this.replaceOpen) this.replaceField.focus()
		else this.findField.focus()
	}

	private commit() {
		const next = new SearchQuery({
			search: this.findField.value,
			replace: this.replaceField.value,
			caseSensitive: this.caseBtn.getAttribute('aria-pressed') === 'true',
			regexp: this.reBtn.getAttribute('aria-pressed') === 'true',
			wholeWord: this.wordBtn.getAttribute('aria-pressed') === 'true',
			test: this.inSelection ? matchInSelection : undefined,
		})
		if (next.eq(this.query)) {
			this.paint()
			return
		}
		this.query = next
		this.view.dispatch({ effects: setSearchQuery.of(next) })
		this.paint()
	}

	private replaceOne() {
		if (this.preserveCase) this.replacePreserving(false)
		else replaceNext(this.view)
	}

	private replaceEvery() {
		if (this.preserveCase) this.replacePreserving(true)
		else replaceAll(this.view)
	}

	private replacePreserving(all: boolean) {
		const q = getSearchQuery(this.view.state)
		if (!q.valid) return
		if (all) {
			const changes: { from: number; to: number; insert: string }[] = []
			const cursor = q.getCursor(this.view.state)
			for (;;) {
				const step = cursor.next()
				if (step.done) break
				const found = this.view.state.sliceDoc(step.value.from, step.value.to)
				changes.push({
					from: step.value.from,
					to: step.value.to,
					insert: preserveCase(found, q.replace),
				})
			}
			if (changes.length) this.view.dispatch({ changes })
			return
		}
		const { from, to } = this.view.state.selection.main
		const cursor = q.getCursor(this.view.state, from, to)
		const step = cursor.next()
		if (step.done) {
			findNext(this.view)
			return
		}
		const found = this.view.state.sliceDoc(step.value.from, step.value.to)
		this.view.dispatch({
			changes: {
				from: step.value.from,
				to: step.value.to,
				insert: preserveCase(found, q.replace),
			},
		})
		findNext(this.view)
	}

	private keydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault()
			closeSearchPanel(this.view)
			this.view.focus()
			return
		}
		if (runScopeHandlers(this.view, e, 'search-panel')) {
			e.preventDefault()
			return
		}
		if (e.key !== 'Enter') return
		e.preventDefault()
		if (e.target === this.replaceField) {
			this.replaceOne()
			return
		}
		;(e.shiftKey ? findPrevious : findNext)(this.view)
	}

	private paint() {
		const q = getSearchQuery(this.view.state)
		if (!q.eq(this.query)) {
			this.query = q
			this.findField.value = q.search
			this.replaceField.value = q.replace
			press(this.caseBtn, q.caseSensitive)
			press(this.reBtn, q.regexp)
			press(this.wordBtn, q.wholeWord)
		}
		press(this.preserveBtn, this.preserveCase)
		press(this.inSelBtn, this.inSelection)

		const invalid = !!q.search && !q.valid
		this.findField.classList.toggle('cm-mystique-search-bad', invalid)
		this.countEl.classList.toggle('cm-mystique-search-count-bad', invalid)
		const stats = matchStats(this.view, q)
		if (invalid) {
			this.countEl.textContent = 'Invalid'
			this.countEl.title = 'Invalid regular expression'
		} else if (!q.search || stats.total === 0) {
			this.countEl.textContent = 'No results'
			this.countEl.title = q.search ? 'No matches' : 'Find in editor'
		} else {
			const total = stats.capped ? `${stats.total}+` : String(stats.total)
			const current = stats.current > 0 ? stats.current : 1
			this.countEl.textContent = `${current} of ${total}`
			this.countEl.title = `${stats.total}${stats.capped ? '+' : ''} matches`
		}
	}
}

function matchInSelection(
	_match: string,
	state: EditorState,
	from: number,
	to: number,
) {
	const ranges = state.selection.ranges.filter((r) => !r.empty)
	if (ranges.length === 0) return true
	return ranges.some((r) => from >= r.from && to <= r.to)
}

function preserveCase(source: string, replacement: string): string {
	if (!replacement) return replacement
	if (source.length > 0 && source === source.toUpperCase()) {
		return replacement.toUpperCase()
	}
	if (source.length > 0 && source === source.toLowerCase()) {
		return replacement.toLowerCase()
	}
	const head = source.charAt(0)
	if (head && head === head.toUpperCase() && head !== head.toLowerCase()) {
		return replacement.charAt(0).toUpperCase() + replacement.slice(1)
	}
	return replacement
}

function matchStats(view: EditorView, query: SearchQuery) {
	if (!query.valid) return { current: 0, total: 0, capped: false }
	const cursor = query.getCursor(view.state)
	const head = view.state.selection.main.head
	let total = 0
	let current = 0
	let capped = false
	for (;;) {
		const step = cursor.next()
		if (step.done) break
		total += 1
		if (step.value.from <= head && head <= step.value.to) current = total
		if (total >= MATCH_CAP) {
			capped = true
			break
		}
	}
	return { current, total, capped }
}

function field(opts: {
	value: string
	placeholder: string
	label: string
	main?: boolean
	oninput: () => void
}): HTMLInputElement {
	const input = el('input', {
		type: 'search',
		class: 'cm-mystique-search-field',
		value: opts.value,
		placeholder: opts.placeholder,
		'aria-label': opts.label,
		autocomplete: 'off',
		spellcheck: 'false',
		form: '',
		...(opts.main ? { 'main-field': 'true' } : {}),
	})
	input.addEventListener('input', opts.oninput)
	return input
}

function toggleBtn(
	label: string,
	title: string,
	on: boolean,
	onToggle: () => void,
	extraClass = '',
): HTMLButtonElement {
	const btn = el('button', {
		type: 'button',
		class: extraClass
			? `cm-mystique-search-toggle ${extraClass}`
			: 'cm-mystique-search-toggle',
		title,
		'aria-label': title,
		'aria-pressed': on ? 'true' : 'false',
	})
	btn.textContent = label
	if (on) btn.classList.add('cm-mystique-search-on')
	btn.addEventListener('click', () => {
		press(btn, btn.getAttribute('aria-pressed') !== 'true')
		onToggle()
	})
	return btn
}

function iconBtn(
	name: IconName,
	title: string,
	onclick: () => void,
): HTMLButtonElement {
	const btn = el('button', {
		type: 'button',
		class: 'cm-mystique-search-icon',
		title,
		'aria-label': title,
	})
	btn.append(icon(name))
	btn.addEventListener('click', onclick)
	return btn
}

function markBtn(
	inner: string,
	title: string,
	on: boolean,
	onclick: () => void,
	toggle = true,
): HTMLButtonElement {
	const btn = el('button', {
		type: 'button',
		class: 'cm-mystique-search-icon',
		title,
		'aria-label': title,
		...(toggle ? { 'aria-pressed': on ? 'true' : 'false' } : {}),
	})
	btn.append(svgMark(inner))
	btn.addEventListener('click', () => {
		if (toggle) press(btn, btn.getAttribute('aria-pressed') !== 'true')
		onclick()
	})
	return btn
}

function press(btn: HTMLButtonElement, on: boolean) {
	btn.setAttribute('aria-pressed', on ? 'true' : 'false')
	btn.classList.toggle('cm-mystique-search-on', on)
}

function icon(name: IconName): SVGSVGElement {
	return svgMark(iconInner[name])
}

function svgMark(inner: string): SVGSVGElement {
	const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg')
	svg.setAttribute('viewBox', '0 0 16 16')
	svg.setAttribute('fill', 'none')
	svg.setAttribute('aria-hidden', 'true')
	svg.innerHTML = inner
	return svg
}

function el<K extends keyof HTMLElementTagNameMap>(
	tag: K,
	attrs: Record<string, string | boolean | number> = {},
	kids: Array<Node | string> = [],
): HTMLElementTagNameMap[K] {
	const node = document.createElement(tag)
	for (const [key, value] of Object.entries(attrs)) {
		if (value === false) continue
		if (value === true) node.setAttribute(key, '')
		else node.setAttribute(key, String(value))
	}
	for (const kid of kids) node.append(kid)
	return node
}