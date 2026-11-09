import assert from 'node:assert/strict'
import { test } from 'vitest'

import { TableEdits } from '../../src/lib/features/workspace/table/edits.svelte.ts'
import {
	tableCellMenu,
	tableCellPick,
	tableRowMenu,
	tableRowPick,
} from '../../src/lib/features/workspace/table/menus.ts'
import { pagerTotal } from '../../src/lib/features/workspace/table/paging.svelte.ts'
import { pasteCellEdits } from '../../src/lib/features/workspace/table/paste-batch.ts'

type Row = Record<string, unknown>

function makeEdits(rows: Row[]) {
	let baseline: Row[] = []
	const edits = new TableEdits({
		columns: () => [{ name: 'id' }, { name: 'name' }],
		baseline: { get: () => baseline, set: (next) => (baseline = next) },
	})
	edits.load(rows, true)
	return { edits, baseline: () => baseline }
}

test('a cell edit is dirty until it equals the saved value again', () => {
	const { edits } = makeEdits([{ id: 1, name: 'a' }])
	edits.setCell(0, 'name', 'b')
	assert.equal(edits.dirtyCount, 1)
	assert.equal(edits.isDirty(0, 'name'), true)
	edits.setCell(0, 'name', 'a')
	assert.equal(edits.dirtyCount, 0)
})

test('an inserted row shifts marks; deleting it again is a no-op', () => {
	const { edits, baseline } = makeEdits([{ id: 1, name: 'a' }, { id: 2, name: 'b' }])
	edits.setCell(1, 'name', 'x')
	edits.markDeleted(0)
	edits.insertTop({ id: null, name: null })
	assert.equal(edits.rows.length, 3)
	assert.equal(baseline().length, 3)
	assert.equal(edits.isDirty(2, 'name'), true)
	assert.equal(edits.isDeleted(1), true)
	assert.equal(edits.isNew(0), true)
	assert.equal(edits.dirtyCount, 3)
	edits.removeLocal(0)
	assert.equal(edits.isDirty(1, 'name'), true)
	assert.equal(edits.isDeleted(0), true)
	assert.equal(edits.pendingDeletes(), 1)
})

test('undo and redo restore rows, baseline and marks', () => {
	const { edits } = makeEdits([{ id: 1, name: 'a' }])
	edits.noteEdit()
	edits.setCell(0, 'name', 'b')
	assert.equal(edits.undo(), true)
	assert.equal(edits.rows[0]?.name, 'a')
	assert.equal(edits.dirtyCount, 0)
	assert.equal(edits.redo(), true)
	assert.equal(edits.rows[0]?.name, 'b')
	assert.equal(edits.redo(), false)
})

test('an in-place load keeps the baseline; a reset load replaces it', () => {
	const { edits, baseline } = makeEdits([{ id: 1, name: 'a' }])
	assert.equal(edits.load([{ id: 1, name: 'z' }], false), null)
	assert.equal(edits.isDirty(0, 'name'), true)
	assert.equal(baseline()[0]?.name, 'a')
	assert.ok(edits.load([{ id: 1, name: 'z' }], true))
	assert.equal(edits.dirtyCount, 0)
	assert.equal(baseline()[0]?.name, 'z')
})

test('reset drops inserts and deletes; a save without reload adopts the rows', () => {
	const { edits } = makeEdits([{ id: 1, name: 'a' }, { id: 2, name: 'b' }])
	edits.insertTop({ id: 3, name: 'c' })
	edits.markDeleted(1)
	const restored = edits.reset()
	assert.deepEqual(restored.map((r) => r.id), [1, 2])
	assert.equal(edits.dirtyCount, 0)

	edits.markDeleted(0)
	edits.setCell(1, 'name', 'B')
	const saved = edits.adoptSaved()
	assert.deepEqual(saved, [{ id: 2, name: 'B' }])
	assert.equal(edits.dirtyCount, 0)
})

const paste = (text: string, extra: Partial<Parameters<typeof pasteCellEdits>[0]> = {}) =>
	pasteCellEdits({
		text,
		targets: [],
		focused: { row: 0, col: 'b' },
		columns: ['a', 'b', 'c'],
		rowCount: 3,
		isDeleted: () => false,
		toValue: (_r, _c, t) => t,
		...extra,
	})

test('a pasted block lands from the focused cell across visible columns', () => {
	const edits = paste('1\t2\t3\n4\t5\t6\n')
	assert.deepEqual(
		edits.map((e) => `${e.row}:${e.col}=${e.value}`),
		['0:b=1', '0:c=2', '1:b=4', '1:c=5'],
	)
})

test('one value fills every target; deleted rows and missing rows are skipped', () => {
	const fill = paste('x', {
		targets: [
			{ row: 0, col: 'a' },
			{ row: 2, col: 'c' },
		],
	})
	assert.equal(fill.length, 2)
	const skip = paste('1\n2\n3\n4', { isDeleted: (r) => r === 1 })
	assert.deepEqual(skip.map((e) => e.row), [0, 2])
	assert.deepEqual(paste('1', { focused: { row: 0, col: 'hidden' } }), [])
})

test('the pager allows one more page while COUNT is deferred', () => {
	assert.equal(pagerTotal({ rowCount: 0, totalKnown: false, loaded: 100, page: 0 }), 101)
	assert.equal(pagerTotal({ rowCount: 0, totalKnown: false, loaded: 40, page: 2 }), 240)
	assert.equal(pagerTotal({ rowCount: 950, totalKnown: true, loaded: 100, page: 0 }), 950)
})

test('table menus: every item has an action and deleted rows offer Undo delete', () => {
	const rowItems = tableRowMenu({ readOnly: false, targets: [1], isDeleted: () => true })
	for (const item of rowItems) assert.ok(tableRowPick(item.id), item.id)
	assert.ok(rowItems.some((i) => i.id === 'undelete'))
	assert.ok(!rowItems.some((i) => i.id === 'delete'))
	const cellItems = tableCellMenu({
		readOnly: true,
		pending: false,
		targets: [0],
		isDeleted: () => false,
		cellCount: 1,
		allNull: false,
	})
	for (const item of cellItems) assert.ok(tableCellPick(item.id), item.id)
	assert.equal(cellItems[0]?.label, 'View in modal')
	assert.deepEqual(tableCellPick('copy-row-where'), { kind: 'copy-rows', format: 'where' })
})
