/** Context menus of the table grid: row number, data cell, and column header. */
import type { ContextMenuItem } from '#core/ui'
import type { CopyFormat } from '#features/workspace/data-grid'

export type TableMenuAction =
	| { kind: 'copy-rows'; format: CopyFormat }
	| { kind: 'copy-cells' }
	| { kind: 'copy-column' }
	| { kind: 'paste' }
	| { kind: 'edit-modal' }
	| { kind: 'set-null' }
	| { kind: 'clone' }
	| { kind: 'delete' }
	| { kind: 'undelete' }
	| { kind: 'details' }

const ROW_COPY: { format: CopyFormat; label: string }[] = [
	{ format: 'copy', label: 'Copy' },
	{ format: 'csv', label: 'Copy as CSV' },
	{ format: 'markdown', label: 'Copy as Markdown' },
	{ format: 'json', label: 'Copy as JSON' },
	{ format: 'insert', label: 'Copy as INSERT' },
	{ format: 'update', label: 'Copy as UPDATE' },
	{ format: 'in', label: 'Copy as IN list' },
	{ format: 'where', label: 'Copy as WHERE' },
]

/** Cell-menu "Copy row …" entries, in menu order. */
const CELL_ROW_COPY: { id: string; format: CopyFormat; label: string }[] = [
	{ id: 'copy-row', format: 'copy', label: 'Copy row' },
	{ id: 'copy-row-csv', format: 'csv', label: 'Copy row as CSV' },
	{ id: 'copy-row-json', format: 'json', label: 'Copy row as JSON' },
	{ id: 'copy-row-markdown', format: 'markdown', label: 'Copy row as Markdown' },
	{ id: 'copy-row-insert', format: 'insert', label: 'Copy row as INSERT' },
	{ id: 'copy-row-update', format: 'update', label: 'Copy row as UPDATE' },
	{ id: 'copy-row-in', format: 'in', label: 'Copy as IN list' },
	{ id: 'copy-row-where', format: 'where', label: 'Copy as WHERE' },
]

/** Duplicate / Delete / Undo delete / Show details, shared by both menus. */
function rowActions(input: {
	readOnly: boolean
	targets: readonly number[]
	isDeleted: (row: number) => boolean
}): ContextMenuItem[] {
	const { readOnly, targets, isDeleted } = input
	const pendingAll = targets.length > 0 && targets.every(isDeleted)
	const items: ContextMenuItem[] = [
		{
			id: 'clone',
			label: 'Duplicate row',
			shortcut: '⌘/Ctrl+D',
			disabled: readOnly || pendingAll,
			separatorBefore: true,
		},
	]
	if (targets.some((i) => !isDeleted(i))) {
		items.push({
			id: 'delete',
			label: 'Delete row',
			shortcut: '⌘/Ctrl+Delete',
			danger: true,
			disabled: readOnly,
		})
	}
	if (targets.some(isDeleted)) {
		items.push({ id: 'undelete', label: 'Undo delete', disabled: readOnly })
	}
	items.push({
		id: 'details',
		label: 'Show details',
		separatorBefore: true,
		disabled: targets.length > 1,
	})
	return items
}

export function tableRowMenu(input: {
	readOnly: boolean
	targets: readonly number[]
	isDeleted: (row: number) => boolean
}): ContextMenuItem[] {
	return [...ROW_COPY.map(({ format, label }) => ({ id: format, label })), ...rowActions(input)]
}

export function tableCellMenu(input: {
	readOnly: boolean
	/** The clicked row is marked for delete. */
	pending: boolean
	targets: readonly number[]
	isDeleted: (row: number) => boolean
	/** Cells an action applies to, and whether all of them are NULL. */
	cellCount: number
	allNull: boolean
}): ContextMenuItem[] {
	const { readOnly, pending, cellCount, allNull } = input
	return [
		{
			id: 'edit-modal',
			label: readOnly ? 'View in modal' : 'Edit Modal',
			shortcut: '⇧Enter',
			disabled: pending || cellCount > 1,
		},
		{
			id: 'set-null',
			label: 'Set NULL',
			shortcut: 'Delete',
			disabled: readOnly || pending || allNull,
		},
		{ id: 'copy', label: 'Copy', separatorBefore: true },
		{ id: 'paste', label: 'Paste', disabled: readOnly || pending },
		...CELL_ROW_COPY.map(({ id, label }, i) =>
			i === 0 ? { id, label, separatorBefore: true } : { id, label },
		),
		{ id: 'copy-column', label: 'Copy column name', separatorBefore: true },
		...rowActions(input),
	]
}

export function tableColumnMenu(frozen: boolean): ContextMenuItem[] {
	return [
		{ id: 'freeze', label: frozen ? 'Unfreeze column' : 'Freeze column' },
		{ id: 'hide', label: 'Hide column' },
		{ id: 'left', label: 'Move left' },
		{ id: 'right', label: 'Move right' },
	]
}

const SHARED: Record<string, TableMenuAction> = {
	clone: { kind: 'clone' },
	delete: { kind: 'delete' },
	undelete: { kind: 'undelete' },
	details: { kind: 'details' },
}

export function tableRowPick(id: string): TableMenuAction | null {
	const copy = ROW_COPY.find((c) => c.format === id)
	if (copy) return { kind: 'copy-rows', format: copy.format }
	return SHARED[id] ?? null
}

export function tableCellPick(id: string): TableMenuAction | null {
	const copy = CELL_ROW_COPY.find((c) => c.id === id)
	if (copy) return { kind: 'copy-rows', format: copy.format }
	if (id === 'copy') return { kind: 'copy-cells' }
	if (id === 'copy-column') return { kind: 'copy-column' }
	if (id === 'paste') return { kind: 'paste' }
	if (id === 'edit-modal') return { kind: 'edit-modal' }
	if (id === 'set-null') return { kind: 'set-null' }
	return SHARED[id] ?? null
}
