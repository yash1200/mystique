/**
 * Where the table grid's page comes from, and when it is replaced.
 *
 * On mount the grid restores the tab's edit buffer or loads `table.rows`.
 * After that the page changes through the server (filter, sort, pager,
 * Refresh, save) or in place (the row inspector). `tableSync` carries the
 * per-tab signals; a signal that arrives mid-edit waits for the edit to end,
 * and a server page never replaces unsaved edits.
 */
import type { TableMeta, TablePreviewFilter, TablePreviewSortDir } from '#core/contracts'
import type { GridSelection } from '#features/workspace/data-grid'
import {
	adoptServerRows,
	getEditBuffer,
	rememberEditBuffer,
	shouldRestoreEditBuffer,
	tabEditsAreDirty,
	type EditBuffer,
} from '#features/workspace/session'
import { untrack } from 'svelte'
import type { TableColumns } from './columns.svelte'
import type { CellEditor } from './editing.svelte'
import type { TableEdits } from './edits.svelte'
import { seedSignature, type PreviewSeed, type TableFilters } from './filters/filter-state.svelte'
import { PAGE_SIZE, type TablePaging } from './paging.svelte'
import { tableSync } from './sync.svelte'

export type ReloadFn = (opts?: {
	offset?: number
	filters?: TablePreviewFilter[]
	codeFilter?: string
	sortColumn?: string
	sortDir?: TablePreviewSortDir
}) => Promise<void>

export type LoaderContext = {
	table: () => TableMeta
	tabId: () => string | null
	connectionId: () => string | null
	/** Fetch a page from the engine; undefined when the grid has no loader. */
	onreload: () => ReloadFn | undefined
	/** The tab's saved preview: filters, code filter and sort to seed the bar with. */
	seed: () => PreviewSeed
	edits: TableEdits
	filters: TableFilters
	paging: TablePaging
	columns: TableColumns
	sel: GridSelection<string>
	editor: CellEditor
	/** Another connection or table: the saver looks up its latest save. */
	forTable: (key: string) => void
	flash: (message: string) => void
	report: (err: unknown) => void
}

export class TableLoader {
	/** The table whose rows the grid holds; null until the first load. */
	loadedTableId = $state<string | null>(null)
	#appliedRows = $state(0)
	#appliedPreview = $state(0)
	#layoutKey = ''
	readonly #ctx: LoaderContext

	rowsVersion = $derived.by(() => tableSync.rowsVersion(this.#ctx.tabId()))
	previewVersion = $derived.by(() => tableSync.previewVersion(this.#ctx.tabId()))

	constructor(ctx: LoaderContext) {
		this.#ctx = ctx
		this.#watch()
	}

	/** The saved baseline moved: the inspector reseeds its drafts. */
	baselineChanged() {
		const tabId = this.#ctx.tabId()
		if (tabId) tableSync.baselineChanged(tabId)
	}

	/** Write rows back into the catalog table (no server reload). */
	replaceTableRows(rows: Record<string, unknown>[]) {
		const table = this.#ctx.table()
		table.rows.splice(0, table.rows.length, ...rows.map((r) => ({ ...r })))
	}

	#restore(buf: EditBuffer) {
		this.#ctx.edits.restore(buf)
		this.#ctx.paging.page = buf.page
		this.loadedTableId = this.#ctx.table().id
		this.#ctx.editor.drop()
	}

	/** Take `table.rows`. `resetBaseline` makes them the saved baseline and clears the selection. */
	loadFromTable(resetBaseline: boolean) {
		const { edits, editor, sel, paging } = this.#ctx
		const table = this.#ctx.table()
		const tabId = this.#ctx.tabId()
		const adopted = edits.load(table.rows, resetBaseline)
		if (adopted && tabId) adoptServerRows(tabId, adopted, paging.page)
		editor.drop()
		this.loadedTableId = table.id
		if (resetBaseline) sel.clear({ cell: true })
	}

	#markServerPreviewApplied() {
		this.#appliedPreview = this.previewVersion
		this.#appliedRows = this.rowsVersion
	}

	/** Fetch a page with the current filters and adopt it. False when there is no loader. */
	async reloadPage(offset = this.#ctx.paging.page * PAGE_SIZE): Promise<boolean> {
		const onreload = this.#ctx.onreload()
		if (!onreload) return false
		await onreload(this.#ctx.filters.reloadOptions(offset))
		this.loadFromTable(true)
		this.#markServerPreviewApplied()
		return true
	}

	#reloadFromStart() {
		const { paging } = this.#ctx
		paging.page = 0
		if (!this.#ctx.onreload()) return
		void paging.run(async () => void (await this.reloadPage(0)), this.#ctx.report)
	}

	/** Filters, sort and paging replace the page, so pending edits must go first. */
	#blockedByEdits(action: string): boolean {
		if (this.#ctx.edits.dirtyCount === 0) return false
		this.#ctx.flash(`Save or reset changes before ${action}.`)
		return true
	}

	cycleSort(column: string) {
		if (this.#blockedByEdits('sorting')) return
		this.#ctx.filters.cycleSort(column)
		this.#reloadFromStart()
	}

	applyFilters() {
		if (this.#blockedByEdits('filtering')) return
		const { filters } = this.#ctx
		if (filters.mode === 'code' && !filters.code.trim()) {
			this.#ctx.flash('Enter a filter')
			return
		}
		filters.active = true
		this.#reloadFromStart()
	}

	clearFilters() {
		if (this.#blockedByEdits('filtering')) return
		this.#ctx.filters.clear()
		this.#reloadFromStart()
	}

	async goToPage(nextPage: number) {
		const { editor, paging } = this.#ctx
		if (editor.editing) editor.commit()
		if (this.#blockedByEdits('changing page')) return
		const p = Math.max(0, Math.min(paging.totalPages - 1, nextPage))
		if (p === paging.safePage) return
		paging.page = p
		if (!this.#ctx.onreload()) return
		await paging.run(async () => void (await this.reloadPage(p * PAGE_SIZE)), this.#ctx.report)
	}

	/** Reload this page. With unsaved edits the first press only arms the discard. */
	async refresh() {
		const { editor, edits, paging } = this.#ctx
		if (paging.refreshing) return
		if (editor.editing) editor.cancel()
		if (edits.dirtyCount > 0 && !paging.refreshArmed) {
			paging.refreshArmed = true
			this.#ctx.flash('Unsaved edits. Refresh again to discard them.')
			return
		}
		paging.refreshArmed = false
		const keepPage = paging.page
		await paging.run(async () => {
			if (!(await this.reloadPage(keepPage * PAGE_SIZE))) {
				const rows = edits.revertToSeed()
				const tabId = this.#ctx.tabId()
				if (tabId) adoptServerRows(tabId, rows, keepPage)
			}
			paging.page = keepPage
			this.baselineChanged()
		}, this.#ctx.report)
	}

	/** Throw away every pending edit (one undo step). */
	resetChanges() {
		const { editor, edits, sel } = this.#ctx
		if (editor.editing) editor.cancel()
		if (edits.dirtyCount === 0) return
		edits.noteEdit()
		this.replaceTableRows(edits.reset())
		sel.setCell(null)
		this.baselineChanged()
	}

	#watch() {
		const ctx = this.#ctx
		const { edits, filters, paging, editor, sel } = ctx

		// A second refresh confirms a discard. Clearing the edits requires a new confirm.
		$effect(() => {
			if (edits.dirtyCount === 0) paging.refreshArmed = false
		})

		$effect(() => {
			void ctx.table().id
			void this.previewVersion
			paging.lastFetchedAt = Date.now()
		})

		// First load: the tab's edit buffer when it still matches, else the table rows.
		$effect(() => {
			const table = ctx.table()
			if (this.loadedTableId === table.id) return
			const key = ctx.tabId()
			if (key) {
				const existing = getEditBuffer(key)
				if (existing && shouldRestoreEditBuffer(existing, table.rows.length)) {
					this.#restore(existing)
					filters.applySeed(ctx.seed())
					this.#appliedRows = this.rowsVersion
					this.#appliedPreview = this.previewVersion
					return
				}
			}
			this.loadFromTable(true)
			paging.page = 0
			filters.applySeed(ctx.seed())
			this.#appliedRows = -1
			this.#appliedPreview = this.previewVersion
		})

		// Keep the session's buffer current so a remount restores the edits.
		$effect(() => {
			const key = ctx.tabId()
			if (!key || this.loadedTableId !== ctx.table().id) return
			const snap = edits.buffer(paging.page)
			untrack(() => rememberEditBuffer(key, snap))
		})

		// FK jump (or explorer re-open) can reseed the same table without remounting.
		$effect(() => {
			const seed = ctx.seed()
			const sig = seedSignature(seed)
			if (this.loadedTableId !== ctx.table().id) return
			if (sig === filters.appliedSeed) return
			// Filter apply writes the same seeds we already have in the bar —
			// don't rebuild rules (that would drop extra empty conditions).
			if (sig === filters.localSignature()) {
				filters.appliedSeed = sig
				return
			}
			filters.applySeed(seed)
		})

		// The inspector wrote into the rows: adopt them, keep the baseline.
		$effect(() => {
			const version = this.rowsVersion
			if (this.loadedTableId !== ctx.table().id) return
			if (version === this.#appliedRows) return
			if (editor.editing) return
			this.#appliedRows = version
			this.loadFromTable(false)
		})

		// Filter / sort / page / open replace the preview page. That is a new
		// saved baseline, not an in-place inspector edit.
		$effect(() => {
			const version = this.previewVersion
			if (this.loadedTableId !== ctx.table().id) return
			if (version === this.#appliedPreview) return
			if (editor.editing) return
			this.#appliedPreview = version
			const tabId = ctx.tabId()
			if (tabId && tabEditsAreDirty(tabId)) return
			this.loadFromTable(true)
			this.#appliedRows = this.rowsVersion
		})

		// Clear cell focus when table reloads
		$effect(() => {
			void ctx.table().id
			untrack(() => sel.clear({ row: true }))
		})

		// Column layout and the latest save follow the connection and table.
		$effect(() => {
			const id = ctx.connectionId()
			const tableId = ctx.table().id
			const key = `${id ?? ''}\n${tableId}`
			if (!id || this.#layoutKey === key) return
			this.#layoutKey = key
			untrack(() => {
				edits.clearHistory()
				ctx.columns.load(id, tableId, key)
				ctx.forTable(key)
			})
		})
	}
}
