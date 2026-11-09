/**
 * "Filter by this value" from a table cell's context menu.
 *
 * NULL offers IS NULL / IS NOT NULL. A value offers = and ≠, unless the grid
 * only holds part of it (a large-cell preview), or the engine cannot compare
 * it as text: bytes outside Postgres (`bytea = '\x…'` is valid there), and
 * documents / arrays on MongoDB. The text `null` is refused too, because the
 * filter compiler reads `= null` as IS NULL.
 */
import type { ContextMenuItem } from '#core/ui'
import { isBinaryValue, isLargeCell } from '#features/workspace/data-grid'
import type { FilterOp } from './filters/types'

export type CellFilterOp = Extract<FilterOp, 'eq' | 'neq' | 'is_null' | 'not_null'>

export type CellFilterValue =
	| { kind: 'null' }
	| { kind: 'value'; text: string }
	| { kind: 'blocked'; reason: string }

const LABEL_MAX = 24

export function cellFilterValue(
	value: unknown,
	columnType: string | null | undefined,
	engine: string,
): CellFilterValue {
	if (value === null || value === undefined) return { kind: 'null' }
	if (isLargeCell(value)) {
		return { kind: 'blocked', reason: 'Only a preview of this value is loaded.' }
	}
	if (isBinaryValue(value, columnType) && engine !== 'postgres') {
		return { kind: 'blocked', reason: 'Binary values cannot be filtered by value here.' }
	}
	if (typeof value === 'object') {
		if (engine === 'mongodb') {
			return { kind: 'blocked', reason: 'Documents and arrays cannot be filtered by value.' }
		}
		return { kind: 'value', text: JSON.stringify(value) }
	}
	const text = typeof value === 'boolean' ? (value ? 'true' : 'false') : String(value)
	if (text.trim().toLowerCase() === 'null') {
		return { kind: 'blocked', reason: 'The text “null” reads as NULL in a filter.' }
	}
	return { kind: 'value', text }
}

function shortValue(text: string): string {
	const flat = text.replace(/\s+/g, ' ')
	return flat.length > LABEL_MAX ? `${flat.slice(0, LABEL_MAX - 1)}…` : flat
}

/** Menu entries (ids `filter-<op>`), first in the cell menu. */
export function cellFilterMenu(column: string, value: CellFilterValue): ContextMenuItem[] {
	if (value.kind === 'null') {
		return [
			{ id: 'filter-is_null', label: `Filter ${column} IS NULL` },
			{ id: 'filter-not_null', label: `Filter ${column} IS NOT NULL` },
		]
	}
	if (value.kind === 'blocked') {
		const why = value.reason.replace(/\.$/, '')
		return [
			{
				id: 'filter-eq',
				label: `Can’t filter: ${why.charAt(0).toLowerCase()}${why.slice(1)}`,
				disabled: true,
			},
		]
	}
	const shown = shortValue(value.text)
	return [
		{ id: 'filter-eq', label: `Filter ${column} = ${shown}` },
		{ id: 'filter-neq', label: `Filter ${column} ≠ ${shown}` },
	]
}

export function cellFilterPick(id: string): CellFilterOp | null {
	switch (id) {
		case 'filter-eq':
			return 'eq'
		case 'filter-neq':
			return 'neq'
		case 'filter-is_null':
			return 'is_null'
		case 'filter-not_null':
			return 'not_null'
		default:
			return null
	}
}
