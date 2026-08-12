/**
 * Live object explorer catalog — loaded from the engine session (Tauri).
 * Falls back is handled by the caller (mock catalog) when not available.
 */
import type {
	CatalogObjectKind,
	ColumnDef,
	DatabaseMeta,
	TableMeta,
} from '$lib/types/workspace'
import {
	fetchEngineTablePreview,
	listEngineCatalog,
	type TablePreviewFilter,
	type TablePreviewResult,
} from './engines'

const MONGO_SYSTEM_DBS = new Set(['admin', 'local', 'config'])

/** Known FE-only mock database names (never come from a real Mongo server seed). */
const MONGO_MOCK_DB_NAMES = new Set(['app', 'analytics'])

function emptyLeaf(
	database: string,
	schema: string,
	name: string,
	kind: CatalogObjectKind,
	rowCount = 0,
): TableMeta {
	const id = schema ? `${database}.${schema}.${name}` : `${database}.${name}`
	return {
		id,
		database,
		schema,
		name,
		kind,
		rowCount,
		columns: [],
		rows: [],
	}
}

/** Prefer the connection’s database, then first non-system name. */
export function preferDatabase(
	names: string[],
	preferred?: string | null,
	systemNames?: Iterable<string> | null,
): string {
	const pref = preferred?.trim() || ''
	if (pref && names.includes(pref)) return pref
	const skip =
		systemNames === undefined || systemNames === null
			? MONGO_SYSTEM_DBS
			: new Set(systemNames)
	const nonSystem = names.filter((n) => !skip.has(n))
	return nonSystem[0] ?? names[0] ?? ''
}

/**
 * True when the explorer is still showing FE mock shells (pre-live-catalog era).
 * Used to force a live reload for old workspace sessions.
 */
export function isMockCatalog(
	engine: string,
	list: DatabaseMeta[] | undefined | null,
): boolean {
	if (!list || list.length === 0) return false
	const e = engine.toLowerCase()
	if (e === 'mongodb') {
		const names = new Set(list.map((d) => d.name))
		// Classic mock: app + analytics (+ admin)
		if (names.has('app') && names.has('analytics')) return true
		// No real user DBs from Docker seed / only mock leaves
		const user = list.filter((d) => !MONGO_SYSTEM_DBS.has(d.name))
		if (user.length > 0 && user.every((d) => MONGO_MOCK_DB_NAMES.has(d.name))) {
			return true
		}
	}
	return false
}

/**
 * Load databases + tables/collections for the explorer.
 * Engine-specific listing lives in the plugin (`list_catalog`).
 */
export async function loadLiveCatalog(
	sessionId: string,
	_engine: string,
	opts?: { preferredDatabase?: string | null },
): Promise<DatabaseMeta[]> {
	const dbs = await listEngineCatalog({
		sessionId,
		preferredDatabase: opts?.preferredDatabase ?? null,
	})
	return dbs.map((db) => ({
		name: db.name,
		meta: db.meta ?? undefined,
		tables: (db.objects ?? []).map((o) => {
			const database = o.database || db.name
			const schema = o.schema || ''
			const kind =
				o.kind === 'view'
					? 'view'
					: o.kind === 'collection'
						? 'collection'
						: 'table'
			return emptyLeaf(database, schema, o.name, kind, o.rowCount ?? 0)
		}),
	}))
}

/**
 * Postgres can only attach objects for the connected database. Keep previously
 * loaded tables for other DBs so switching the picker does not wipe them.
 */
export function mergeCatalogKeepCached(
	prev: DatabaseMeta[] | undefined,
	live: DatabaseMeta[],
): DatabaseMeta[] {
	if (!prev?.length) return live
	const prevByName = new Map(prev.map((d) => [d.name, d]))
	return live.map((db) => {
		if (db.tables.length > 0) return db
		const cached = prevByName.get(db.name)
		if (cached && cached.tables.length > 0) {
			return { ...db, tables: cached.tables, meta: cached.meta ?? db.meta }
		}
		return db
	})
}

/** Fetch a page of rows for a table/collection and merge into TableMeta. */
export async function loadTablePreview(
	sessionId: string,
	_engine: string,
	table: TableMeta,
	maxRows = 100,
	offset = 0,
	filters: TablePreviewFilter[] = [],
): Promise<TableMeta> {
	const preview = await fetchEngineTablePreview({
		sessionId,
		table: table.name,
		schema: table.schema || null,
		database: table.database || null,
		maxRows,
		offset,
		filters,
	})
	return previewToTableMeta(table, preview)
}

/** Keep PKs / ObjectIds usable for updates & deletes (not stringified blobs). */
function unwrapPreviewCell(v: unknown): unknown {
	if (v == null) return null
	if (typeof v !== 'object') return v
	const o = v as Record<string, unknown>
	if (typeof o.$oid === 'string') return o.$oid
	if (typeof o.$uuid === 'string') return o.$uuid
	if (typeof o.$numberLong === 'string') {
		const n = Number(o.$numberLong)
		return Number.isSafeInteger(n) ? n : o.$numberLong
	}
	if (typeof o.$numberInt === 'string') return Number(o.$numberInt)
	if ('$date' in o) {
		const d = o.$date
		if (typeof d === 'string') return d
		if (d && typeof d === 'object' && '$numberLong' in (d as object)) {
			return new Date(
				Number((d as { $numberLong: string }).$numberLong),
			).toISOString()
		}
	}
	try {
		return JSON.stringify(v)
	} catch {
		return v
	}
}

function previewToTableMeta(
	base: TableMeta,
	preview: TablePreviewResult,
): TableMeta {
	const columns: ColumnDef[] =
		preview.columns.length > 0
			? preview.columns.map((c) => ({
					name: c.name,
					type: c.dataType || 'unknown',
					nullable: c.nullable,
					pk: !!c.pk,
					enumValues:
						c.enumValues && c.enumValues.length > 0 ? c.enumValues : undefined,
					ref: c.references
						? {
								database: c.references.database ?? null,
								schema: c.references.schema ?? null,
								table: c.references.table,
								column: c.references.column,
							}
						: undefined,
				}))
			: base.columns

	const rows: Record<string, unknown>[] = preview.rows.map((cells) => {
		const obj: Record<string, unknown> = {}
		columns.forEach((c, i) => {
			const v = cells[i]
			obj[c.name] = unwrapPreviewCell(v)
		})
		return obj
	})

	const pageCount = preview.rowCount
	const total = preview.totalRows
	const rowCount =
		typeof total === 'number' ? total : Math.max(base.rowCount ?? 0, pageCount)

	return {
		...base,
		columns: columns.length > 0 ? columns : base.columns,
		rows,
		rowCount,
	}
}
