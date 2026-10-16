import { describe, expect, it } from 'vitest'
import type { TableDefinition } from '../../src/lib/core/contracts/ddl'
import {
	addColumn,
	draftFingerprint,
	dropColumn,
	pairRename,
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
})
