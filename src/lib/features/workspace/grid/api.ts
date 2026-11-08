/**
 * Collect grid edits and hand them to the engine plugin.
 * Dialects / execution live in Rust (`apply_table_edits`).
 */
import type { ApplyTableEditsResult, TableMeta } from '#core/contracts'
import {
	applyEngineTableEdits,
	distinctEngineColumnValues,
	invoke,
	isTauri,
	rollbackEngineTableEdits,
} from '#core/ipc'
import { runWithWriteGuard } from '#features/workspace/session'
import { identityCell } from './identity-cell'
import {
	layoutToStored,
	storedToLayout,
	type GridColumnLayout,
	type StoredColumnLayout,
} from './column-layout'
import { DISTINCT_LIMIT } from './column-complete'
export type PersistResult = ApplyTableEditsResult
export { identityCell } from './identity-cell'

function cellEmpty(v: unknown): boolean {
	return v === null || v === undefined || v === ''
}

function asJsonMap(row: Record<string, unknown>): Record<string, unknown> {
	const out: Record<string, unknown> = {}
	for (const [k, v] of Object.entries(row)) {
		out[k] = v === undefined ? null : identityCell(v)
	}
	return out
}

export async function persistTableEdits(input: {
	sessionId: string
	engine: string
	table: TableMeta
	rows: Record<string, unknown>[]
	originals: Record<string, unknown>[]
	newRowIndices: Set<number>
	deletedIndices: Set<number>
	dirtyKeys: Set<string>
	dryRun?: boolean
}): Promise<PersistResult> {
	if (!input.sessionId) {
		throw new Error('No live engine session. Reconnect and try again.')
	}

	const dirtyByRow = new Map<number, string[]>()
	for (const key of input.dirtyKeys) {
		const sep = key.indexOf('::')
		if (sep < 0) continue
		const index = Number(key.slice(0, sep))
		const col = key.slice(sep + 2)
		if (!Number.isFinite(index) || !col) continue
		const list = dirtyByRow.get(index) ?? []
		list.push(col)
		dirtyByRow.set(index, list)
	}

	const updates: {
		original: Record<string, unknown>
		values: Record<string, unknown>
	}[] = []
	const inserts: { values: Record<string, unknown> }[] = []
	const deletes: { original: Record<string, unknown> }[] = []

	for (const index of input.newRowIndices) {
		if (input.deletedIndices.has(index)) continue
		const next = input.rows[index]
		if (!next) continue
		const values: Record<string, unknown> = {}
		for (const col of input.table.columns) {
			const v = next[col.name]
			if (cellEmpty(v)) continue
			values[col.name] = v
		}
		inserts.push({ values })
	}

	for (const index of input.deletedIndices) {
		if (input.newRowIndices.has(index)) continue
		const original = input.originals[index]
		if (!original) continue
		deletes.push({ original: asJsonMap(original) })
	}

	for (const [index, changed] of dirtyByRow) {
		if (input.newRowIndices.has(index) || input.deletedIndices.has(index))
			continue
		const next = input.rows[index]
		const original = input.originals[index]
		if (!next || !original || changed.length === 0) continue
		const values: Record<string, unknown> = {}
		for (const name of changed) {
			values[name] = next[name] === undefined ? null : next[name]
		}
		updates.push({ original: asJsonMap(original), values })
	}

	if (updates.length === 0 && inserts.length === 0 && deletes.length === 0) {
		return { updated: 0, inserted: 0, deleted: 0, statements: [] }
	}

	const request = {
		sessionId: input.sessionId,
		table: input.table.name,
		schema: input.table.schema || null,
		database: input.table.database || null,
		columns: input.table.columns.map((c) => ({
			name: c.name,
			dataType: c.type,
			nullable: !!c.nullable,
			pk: !!c.pk,
		})),
		updates,
		inserts,
		deletes,
	}

	if (input.dryRun) {
		return applyEngineTableEdits({ ...request, dryRun: true })
	}
	return runWithWriteGuard(
		input.sessionId,
		(confirmObjectName) =>
			applyEngineTableEdits({
				...request,
				dryRun: false,
				confirmObjectName: confirmObjectName ?? null,
			}),
		{ confirmLabel: 'Save' },
	)
}

export async function rollbackTableEdits(input: {
	sessionId: string
	saveId: string
}): Promise<void> {
	await runWithWriteGuard(input.sessionId, (confirmObjectName) =>
		rollbackEngineTableEdits({
			sessionId: input.sessionId,
			saveId: input.saveId,
			confirmObjectName: confirmObjectName ?? null,
		}),
	)
}

export async function distinctColumnValues(input: {
	sessionId: string
	table: TableMeta
	column: string
}): Promise<string[]> {
	const values = await distinctEngineColumnValues({
		sessionId: input.sessionId,
		table: input.table.name,
		schema: input.table.schema || null,
		database: input.table.database || null,
		column: input.column,
		limit: DISTINCT_LIMIT,
	})
	const out: string[] = []
	for (const value of values) {
		if (value == null) continue
		out.push(typeof value === 'string' ? value : JSON.stringify(value))
		if (out.length >= DISTINCT_LIMIT) break
	}
	return out
}

export async function loadColumnLayout(
	connectionId: string,
	tableId: string,
): Promise<GridColumnLayout | null> {
	if (!isTauri()) return null
	const row = await invoke<StoredColumnLayout | null>('get_grid_column_layout', {
		connectionId,
		tableId,
	})
	return row ? storedToLayout(row) : null
}

export async function saveColumnLayout(
	connectionId: string,
	tableId: string,
	layout: GridColumnLayout,
): Promise<void> {
	if (!isTauri()) return
	await invoke('save_grid_column_layout', {
		layout: layoutToStored(connectionId, tableId, layout),
	})
}

/** Latest save on this table that can still be rolled back. */
export async function loadLatestSaveId(
	connectionId: string,
	table: TableMeta,
): Promise<string | null> {
	if (!isTauri()) return null
	const row = await invoke<{ saveId?: string } | null>('latest_grid_save', {
		connectionId,
		database: table.database || null,
		schema: table.schema || null,
		table: table.name,
	})
	return row?.saveId ?? null
}
