/**
 * Collect grid edits and hand them to the engine plugin.
 * Dialects / execution live in Rust (`apply_table_edits`).
 */
import type { ApplyTableEditsResult, TableMeta } from '#core/contracts'
import { applyEngineTableEdits } from '#core/ipc'
import { runWithWriteGuard } from '#features/workspace/shell'
import { identityCell } from './identity-cell'
export type PersistResult = ApplyTableEditsResult
export { identityCell } from './identity-cell'

function invokeError(err: unknown): string {
	if (err instanceof Error) return err.message
	if (typeof err === 'string') return err
	if (err && typeof err === 'object' && 'message' in err) {
		return String((err as { message: unknown }).message)
	}
	try {
		return JSON.stringify(err)
	} catch {
		return String(err)
	}
}

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
	profileReadOnly: boolean
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
		return { updated: 0, inserted: 0, deleted: 0 }
	}

	try {
		return await runWithWriteGuard(input.sessionId, (confirmObjectName) =>
			applyEngineTableEdits({
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
				profileReadOnly: input.profileReadOnly,
				confirmObjectName: confirmObjectName ?? null,
			}),
		)
	} catch (err) {
		throw new Error(invokeError(err))
	}
}
