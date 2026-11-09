import { describe, expect, test } from 'vitest'

import {
	cellFilterMenu,
	cellFilterPick,
	cellFilterValue,
} from '../../src/lib/features/workspace/table/cell-filter'
import { TableFilters } from '../../src/lib/features/workspace/table/filters/filter-state.svelte'
import type { TableMeta } from '../../src/lib/core/contracts'

const table = {
	id: 't',
	name: 't',
	columns: [
		{ name: 'id', type: 'int4' },
		{ name: 'email', type: 'text' },
	],
} as unknown as TableMeta

function filters() {
	return new TableFilters({
		table: () => table,
		sessionId: () => null,
		supportsDistinct: () => false,
	})
}

describe('filter by cell value', () => {
	test('values, NULL, and what cannot be filtered', () => {
		expect(cellFilterValue(null, 'text', 'postgres')).toEqual({ kind: 'null' })
		expect(cellFilterValue(42, 'int4', 'mysql')).toEqual({ kind: 'value', text: '42' })
		expect(cellFilterValue(true, 'bool', 'postgres')).toEqual({ kind: 'value', text: 'true' })
		expect(cellFilterValue({ a: 1 }, 'jsonb', 'postgres')).toEqual({
			kind: 'value',
			text: '{"a":1}',
		})
		expect(cellFilterValue({ a: 1 }, 'object', 'mongodb').kind).toBe('blocked')
		// A large-cell preview is not the value.
		expect(
			cellFilterValue({ $large: true, preview: 'abc', totalChars: 9000 }, 'text', 'postgres')
				.kind,
		).toBe('blocked')
		// bytea = '\x…' works on Postgres only.
		expect(cellFilterValue('\\x0102', 'bytea', 'postgres').kind).toBe('value')
		expect(cellFilterValue('\\x0102', 'blob', 'mysql').kind).toBe('blocked')
		expect(cellFilterValue('NULL', 'text', 'postgres').kind).toBe('blocked')
	})

	test('menu entries and picks', () => {
		const items = cellFilterMenu('email', { kind: 'value', text: 'a@b.c' })
		expect(items.map((i) => i.label)).toEqual(['Filter email = a@b.c', 'Filter email ≠ a@b.c'])
		const nulls = cellFilterMenu('email', { kind: 'null' })
		expect(nulls.map((i) => cellFilterPick(i.id))).toEqual(['is_null', 'not_null'])
		const blocked = cellFilterMenu('email', { kind: 'blocked', reason: 'Nope.' })
		expect(blocked[0]?.disabled).toBe(true)
		const long = cellFilterMenu('email', { kind: 'value', text: 'x'.repeat(80) })
		expect(long[0]?.label.length).toBeLessThan(50)
		expect(cellFilterPick('copy')).toBeNull()
	})

	test('a cell rule joins the applied rules and replaces an empty draft', () => {
		const f = filters()
		expect(f.addCellRule('email', 'eq', 'a@b.c')).toBeNull()
		expect(f.rules.map((r) => [r.column, r.op, r.value])).toEqual([['email', 'eq', 'a@b.c']])
		f.active = true
		f.addCellRule('id', 'not_null', '')
		expect(f.rules.map((r) => [r.column, r.op])).toEqual([
			['email', 'eq'],
			['id', 'not_null'],
		])
		// The same rule twice is kept once.
		f.addCellRule('id', 'not_null', '')
		expect(f.rules).toHaveLength(2)
	})

	test('code mode appends a predicate; JSON code is left alone', () => {
		const f = filters()
		f.mode = 'code'
		f.code = "status = 'open'"
		expect(f.addCellRule('email', 'neq', "o'brien")).toBeNull()
		expect(f.code).toBe("status = 'open'\nAND email != 'o'brien'")
		f.code = '{ "email": "a" }'
		expect(f.addCellRule('id', 'eq', '1')).toMatch(/code filter/)
	})
})
