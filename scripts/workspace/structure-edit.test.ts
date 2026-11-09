import { describe, expect, it } from 'vitest'
import type { SchemaChange, TableDefinition } from '../../src/lib/core/contracts/ddl'
import {
	addColumn,
	addedColumns,
	draftFingerprint,
	dropColumn,
	emptySelection,
	keepColumnsApart,
	pairColumns,
	pairRename,
	setChangeSelected,
	setDropIncluded,
	sourceTableOf,
	unpairColumns,
	unpairedNames,
} from '../../src/lib/features/workspace/explorer/structure-edit'

function orders(): TableDefinition {
	return {
		name: 'orders',
		kind: 'table',
		columns: [
			{
				name: 'id',
				dataType: 'integer',
				nullable: false,
				pkPosition: 1,
				original: {
					name: 'id',
					dataType: 'integer',
					nullable: false,
					pkPosition: 1,
				},
			},
		],
	}
}

describe('structure draft edits', () => {
	it('adds a nullable column without writing SQL', () => {
		const next = addColumn(orders())
		const added = next.columns?.at(-1)
		expect(added).toMatchObject({
			name: '',
			dataType: 'text',
			nullable: true,
			drop: false,
		})
		expect(added?.original).toBeUndefined()
		expect(draftFingerprint(next)).not.toBe(draftFingerprint(orders()))
	})

	it('marks an existing column dropped and removes a new one', () => {
		const withNote = addColumn(orders())
		const note = withNote.columns?.at(-1)
		expect(note).toBeTruthy()
		if (!withNote.columns || !note) return
		withNote.columns[withNote.columns.length - 1] = { ...note, name: 'note' }
		const droppedNew = dropColumn(withNote, withNote.columns.length - 1)
		expect(droppedNew.columns?.some((column) => column.name === 'note')).toBe(false)

		const droppedId = dropColumn(orders(), 0)
		expect(droppedId.columns?.[0]).toMatchObject({ name: 'id', drop: true })
	})

	it('does not pair different table names unless asked', () => {
		const names = unpairedNames(['orders', 'lines'], ['orders', 'order_lines'], [])
		expect(names).toEqual({
			sourceOnly: ['lines'],
			targetOnly: ['order_lines'],
		})
		const paired = pairRename([], 'lines', 'order_lines')
		expect(paired).toEqual([{ from: 'lines', to: 'order_lines' }])
		expect(pairRename(paired, 'lines', 'other')).toEqual(paired)
		const after = unpairedNames(['orders', 'lines'], ['orders', 'order_lines'], paired)
		expect(after).toEqual({ sourceOnly: [], targetOnly: [] })
	})

	it('keeps column pairs apart from table pairs', () => {
		const columns = pairColumns([], 'orders', 'title', 'headline')
		expect(columns).toEqual([
			{ from: 'title', to: 'headline', table: 'orders', keepApart: false },
		])
		// A column pair does not hide tables from table pairing.
		expect(unpairedNames(['title'], ['headline'], columns)).toEqual({
			sourceOnly: ['title'],
			targetOnly: ['headline'],
		})
		expect(pairRename(columns, 'title', 'headline')).toHaveLength(2)
		// Re-pairing a column replaces its old pair.
		const repaired = pairColumns(columns, 'orders', 'title', 'name')
		expect(repaired).toEqual([{ from: 'title', to: 'name', table: 'orders', keepApart: false }])
		const apart = keepColumnsApart(repaired, 'orders', 'title', 'name')
		expect(apart).toEqual([{ from: 'title', to: 'name', table: 'orders', keepApart: true }])
		expect(unpairColumns(apart, 'orders', 'title', 'name')).toEqual([])
	})
})

function change(action: string, draft: Partial<TableDefinition> = {}): SchemaChange {
	return {
		name: 'orders',
		action,
		plan: { statements: [], warnings: [], transactional: false, requiresRebuild: false },
		draft: { name: 'orders', ...draft },
		selected: action !== 'drop',
		drops: [],
		columnRenames: [],
	}
}

describe('schema diff selection', () => {
	it('starts with no drops and skips by change name', () => {
		const empty = emptySelection()
		expect(empty).toEqual({ skip: [], drops: [] })
		const skipped = setChangeSelected(empty, change('alter'), false)
		expect(skipped.skip).toEqual(['orders'])
		expect(setChangeSelected(skipped, change('alter'), true).skip).toEqual([])
	})

	it('a table drop is opt-in through the drop list', () => {
		const chosen = setChangeSelected(emptySelection(), change('drop'), true)
		expect(chosen.drops).toEqual([{ table: 'orders', kind: 'table', name: 'orders' }])
		expect(setChangeSelected(chosen, change('drop'), false).drops).toEqual([])
		const column = { table: 'orders', kind: 'column', name: 'legacy' }
		const withColumn = setDropIncluded(emptySelection(), column, true)
		expect(setDropIncluded(withColumn, column, true).drops).toEqual([column])
		expect(setDropIncluded(withColumn, column, false).drops).toEqual([])
	})

	it('names the source table and the columns a change adds', () => {
		const renamed = change('alter', {
			renameTo: 'orders_v2',
			columns: [
				{ name: 'id', dataType: 'int', nullable: false, original: { name: 'id', dataType: 'int', nullable: false } },
				{ name: 'note', dataType: 'text', nullable: true },
				{ name: 'gone', dataType: 'text', nullable: true, drop: true },
			],
		})
		expect(sourceTableOf(renamed)).toBe('orders_v2')
		expect(sourceTableOf(change('alter'))).toBe('orders')
		expect(addedColumns(renamed)).toEqual(['note'])
		expect(addedColumns(change('create', renamed.draft))).toEqual([])
	})
})
