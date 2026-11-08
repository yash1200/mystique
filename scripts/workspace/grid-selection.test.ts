import assert from 'node:assert/strict'
import { test } from 'vitest'

import {
	gridKeyIntent,
	isTypingTarget,
} from '../../src/lib/features/workspace/data-grid/keyboard.ts'
import { GridSelection } from '../../src/lib/features/workspace/data-grid/selection.svelte.ts'
import {
	actionCells,
	cellKey,
	cellRect,
	collapse,
	emptySelection,
	exclusiveCell,
	exclusiveRow,
	moveSelection,
	remapRows,
	rowRange,
	selectedBlock,
	selectionValues,
	toggleRow,
} from '../../src/lib/features/workspace/data-grid/selection-model.ts'

const ORDER = ['id', 'name', 'email']
const keys = (s: { cells: ReadonlyMap<string, unknown> }) => [...s.cells.keys()].sort()

test('Shift range runs from the anchor and keeps it', () => {
	let s = exclusiveRow(emptySelection<string>(), 3)
	s = rowRange(s, 6)
	assert.deepEqual([...s.rows].sort(), [3, 4, 5, 6])
	assert.equal(s.rowAnchor, 3)
	assert.equal(s.row, 6)
	s = rowRange(s, 1)
	assert.deepEqual([...s.rows].sort(), [1, 2, 3])
})

test('a toggle never leaves the row set empty', () => {
	let s = exclusiveRow(emptySelection<string>(), 2)
	s = toggleRow(s, 2)
	assert.deepEqual([...s.rows], [2])
	s = toggleRow(s, 4)
	assert.deepEqual([...s.rows].sort(), [2, 4])
})

test('a cell rectangle follows the visible order, not the data order', () => {
	const visible = ['email', 'id', 'name']
	let s = exclusiveCell(emptySelection<string>(), 0, 'email')
	s = cellRect(s, 1, 'id', visible)
	assert.deepEqual(keys(s), [cellKey(0, 'email'), cellKey(0, 'id'), cellKey(1, 'email'), cellKey(1, 'id')].sort())
	assert.equal(s.kind, 'cells')
	assert.deepEqual([...s.rows], [])
})

test('numeric columns work the same way (query results)', () => {
	let s = exclusiveCell(emptySelection<number>(), 10, 2)
	s = cellRect(s, 11, 0, [0, 1, 2, 3])
	assert.equal(s.cells.size, 6)
	assert.equal(s.cell?.col, 0)
})

test('Escape shrinks a multi-selection, then reports nothing to shrink', () => {
	let s = exclusiveCell(emptySelection<string>(), 0, 'id')
	s = cellRect(s, 2, 'name', ORDER)
	const shrunk = collapse(s)
	assert.ok(shrunk)
	assert.deepEqual(keys(shrunk), [cellKey(2, 'name')])
	assert.equal(collapse(shrunk), null)
})

test('arrows step through visible columns and clamp to the page', () => {
	let s = exclusiveCell(emptySelection<string>(), 0, 'id')
	const bounds = { first: 0, count: 3 }
	let moved = moveSelection(s, { dRow: 0, dCol: 1, extend: false }, ORDER, bounds)
	assert.deepEqual([moved?.row, moved?.col], [0, 'name'])
	s = moved!.state
	moved = moveSelection(s, { dRow: 5, dCol: 0, extend: false }, ORDER, bounds)
	assert.deepEqual([moved?.row, moved?.col], [2, 'name'])
	moved = moveSelection(moved!.state, { dRow: -1, dCol: 1, extend: true }, ORDER, bounds)
	assert.deepEqual(keys(moved!.state).length, 4)
})

test('arrows in row mode stay rows unless they go sideways', () => {
	const s = exclusiveRow(emptySelection<number>(), 100)
	const bounds = { first: 100, count: 50 }
	const down = moveSelection(s, { dRow: 1, dCol: 0, extend: false }, [0, 1], bounds)
	assert.equal(down?.state.kind, 'rows')
	assert.deepEqual([...down!.state.rows], [101])
	const side = moveSelection(s, { dRow: 0, dCol: 1, extend: false }, [0, 1], bounds)
	assert.equal(side?.state.kind, 'cells')
	assert.deepEqual([side?.row, side?.col], [100, 1])
	assert.equal(moveSelection(s, { dRow: 1, dCol: 0, extend: false }, [], bounds), null)
})

test('a removed row drops its focus and the anchor falls back to the new focus', () => {
	let s = exclusiveRow(emptySelection<string>(), 2)
	s = rowRange(s, 4)
	const removed = remapRows(s, (i) => (i === 2 ? null : i > 2 ? i - 1 : i))
	assert.deepEqual([...removed.rows].sort(), [2, 3])
	assert.equal(removed.row, 3)
	assert.equal(removed.rowAnchor, 3)
	const shifted = remapRows(exclusiveCell(emptySelection<string>(), 0, 'id'), (i) => i + 1)
	assert.deepEqual(keys(shifted), [cellKey(1, 'id')])
	assert.equal(shifted.cellAnchor?.row, 1)
})

test('row actions use visible columns only; the copy block keeps holes blank', () => {
	let s = exclusiveCell(emptySelection<string>(), 0, 'id')
	s = { ...s, cells: new Map([...s.cells, [cellKey(1, 'email'), { row: 1, col: 'email' }]]) }
	const block = selectedBlock(s, ['id', 'email'])
	assert.deepEqual(block?.rows, [0, 1])
	assert.deepEqual(block?.cols, ['id', 'email'])
	assert.equal(block?.has(0, 'email'), false)
	const rows = exclusiveRow(emptySelection<string>(), 5)
	assert.deepEqual(
		actionCells(rows, ['name']).map((c) => `${c.row}:${c.col}`),
		['5:name'],
	)
})

test('selection values feed the stats from cells, rows, or the focused cell', () => {
	const data: Record<number, Record<string, number>> = { 0: { id: 1, name: 2 }, 1: { id: 3, name: 4 } }
	const cell = (r: number, c: string) => data[r]?.[c]
	const row = (r: number) => Object.values(data[r] ?? {})
	const rows = rowRange(exclusiveRow(emptySelection<string>(), 0), 1)
	assert.deepEqual(selectionValues(rows, cell, row).sort(), [1, 2, 3, 4])
	const one = exclusiveCell(emptySelection<string>(), 1, 'name')
	assert.deepEqual(selectionValues(one, cell, row), [4])
})

test('GridSelection wraps the model and reads the live column order', () => {
	let order = ['a', 'b', 'c']
	const sel = new GridSelection<string>(() => order)
	sel.pointCell(0, 'a')
	sel.pointCell(1, 'c', { shiftKey: true, metaKey: false, ctrlKey: false })
	assert.equal(sel.cells.size, 6)
	order = ['a', 'c']
	assert.equal(sel.actionCells().length, 4)
	sel.selectAll([4, 5, 6], true)
	assert.equal(sel.kind, 'rows')
	assert.equal(sel.isRowSelected(5), true)
	assert.equal(sel.collapse(), true)
	sel.clear()
	assert.equal(sel.kind, 'none')
})

const key = (k: string, mods: Partial<{ meta: boolean; ctrl: boolean; alt: boolean; shift: boolean }> = {}) => ({
	key: k,
	metaKey: !!mods.meta,
	ctrlKey: !!mods.ctrl,
	altKey: !!mods.alt,
	shiftKey: !!mods.shift,
})

test('grid keys map to one intent each', () => {
	assert.deepEqual(gridKeyIntent(key('ArrowDown', { shift: true })), {
		kind: 'move',
		dRow: 1,
		dCol: 0,
		extend: true,
	})
	assert.deepEqual(gridKeyIntent(key('Tab', { shift: true })), {
		kind: 'move',
		dRow: 0,
		dCol: -1,
		extend: false,
	})
	assert.equal(gridKeyIntent(key('c', { meta: true }))?.kind, 'copy')
	assert.equal(gridKeyIntent(key('a', { ctrl: true }))?.kind, 'select-all')
	assert.equal(gridKeyIntent(key('Enter', { shift: true }))?.kind, 'expand')
	assert.equal(gridKeyIntent(key('Enter'))?.kind, 'edit')
	assert.equal(gridKeyIntent(key('F2'))?.kind, 'edit')
	assert.equal(gridKeyIntent(key('Backspace', { meta: true }))?.kind, 'delete-rows')
	assert.equal(gridKeyIntent(key('Delete'))?.kind, 'set-null')
	assert.equal(gridKeyIntent(key('Delete', { alt: true })), null)
	assert.deepEqual(gridKeyIntent(key('x')), { kind: 'type', key: 'x' })
	assert.equal(gridKeyIntent(key('x', { meta: true })), null)
	assert.equal(gridKeyIntent(key('Escape'))?.kind, 'escape')
})

test('copy and select-all leave Shift and Alt chords to the keymap', () => {
	assert.equal(gridKeyIntent(key('c', { meta: true, shift: true })), null)
	assert.equal(gridKeyIntent(key('a', { meta: true, alt: true })), null)
})

test('text fields keep their keys', () => {
	assert.equal(isTypingTarget(document.createElement('input')), true)
	assert.equal(isTypingTarget(document.createElement('div')), false)
	assert.equal(isTypingTarget(null), false)
})
