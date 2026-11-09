/**
 * Column layout of one table tab (order, frozen, hidden, widths), stored per
 * connection and table, and the column header menu that edits it.
 */
import type { TableMeta } from '#core/contracts'
import {
	EMPTY_COLUMN_LAYOUT,
	moveColumn,
	toggleFrozen,
	toggleHidden,
	visibleColumns,
	withColumnWidth,
	type GridColumnLayout,
} from '#features/workspace/data-grid'
import { loadColumnLayout, saveColumnLayout } from './api'
import { tableColumnMenu } from './menus'

export class TableColumns {
	layout = $state<GridColumnLayout>({ ...EMPTY_COLUMN_LAYOUT })
	/** Open header menu. */
	menu = $state<{ column: string; x: number; y: number } | null>(null)
	#key = ''
	readonly #table: () => TableMeta
	readonly #connectionId: () => string | null

	constructor(opts: { table: () => TableMeta; connectionId: () => string | null }) {
		this.#table = opts.table
		this.#connectionId = opts.connectionId
	}

	/** Visible order (frozen, then the rest; no hidden). Every grid operation maps through this. */
	visible = $derived.by(() => visibleColumns(this.#table().columns, this.layout))

	menuItems = $derived(
		this.menu ? tableColumnMenu(this.layout.frozen.includes(this.menu.column)) : [],
	)

	isHidden(name: string): boolean {
		return this.layout.hidden.includes(name)
	}

	/** Apply and store a layout. */
	change(next: GridColumnLayout) {
		this.layout = next
		const id = this.#connectionId()
		if (!id) return
		void saveColumnLayout(id, this.#table().id, next).catch(() => {})
	}

	setWidth(column: string, width: number) {
		this.change(withColumnWidth(this.layout, column, width))
	}

	openMenu(column: string, event: MouseEvent) {
		this.menu = { column, x: event.clientX, y: event.clientY }
	}

	pickMenu(id: string) {
		const name = this.menu?.column
		this.menu = null
		if (!name) return
		const names = this.#table().columns.map((column) => column.name)
		if (id === 'freeze') this.change(toggleFrozen(this.layout, name))
		if (id === 'hide') this.change(toggleHidden(this.layout, name))
		if (id === 'left') this.change(moveColumn(this.layout, names, name, -1))
		if (id === 'right') this.change(moveColumn(this.layout, names, name, 1))
	}

	/** Load the stored layout for this connection and table. */
	load(connectionId: string, tableId: string, key: string) {
		this.#key = key
		void loadColumnLayout(connectionId, tableId)
			.then((layout) => {
				if (this.#key !== key) return
				// No stored layout: start clean, not with the last table's widths.
				this.layout = layout ?? { ...EMPTY_COLUMN_LAYOUT, widths: {} }
			})
			.catch(() => {})
	}
}
