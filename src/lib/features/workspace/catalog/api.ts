/**
 * Live object explorer catalog — loaded from the engine session (Tauri).
 */
import type {
	CatalogObjectKind,
	ColumnDef,
	DatabaseMeta,
	TableMeta,
	TablePreviewFilter,
	TablePreviewResult,
	TablePreviewSort,
	TablePreviewStats,
} from '$core/contracts'
import {
	fetchEngineTablePreview,
	fetchEngineTablePreviewStats,
	listEngineCatalog,
} from '$core/ipc'
const MONGO_SYSTEM_DBS = new Set(['admin', 'local', 'config'])

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
 * Load databases + tables/collections for the explorer.
 * Engine-specific listing lives in the plugin (`list_catalog`).
 */
export async function loadLiveCatalog(
	sessionId: string,
	_engine: string,
	opts?: { preferredDatabase?: string | null; includeObjects?: boolean },
): Promise<DatabaseMeta[]> {
	const dbs = await listEngineCatalog({
		sessionId,
		preferredDatabase: opts?.preferredDatabase ?? null,
		includeObjects: opts?.includeObjects,
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
/** Keep loaded preview (columns/rows) when a catalog refresh returns empty leaves. */
function preserveTablePreviews(
	prev: TableMeta[] | undefined,
	live: TableMeta[],
): TableMeta[] {
	if (!prev?.length) return live
	const prevById = new Map(prev.map((t) => [t.id, t]))
	return live.map((t) => {
		const p = prevById.get(t.id)
		if (!p) return t
		if (t.columns.length === 0 && p.columns.length > 0) {
			return {
				...t,
				columns: p.columns,
				rows: p.rows,
				rowCount: p.rowCount,
			}
		}
		return t
	})
}

export function mergeCatalogKeepCached(
	prev: DatabaseMeta[] | undefined,
	live: DatabaseMeta[],
): DatabaseMeta[] {
	if (!prev?.length) return live
	const prevByName = new Map(prev.map((d) => [d.name, d]))
	return live.map((db) => {
		const cached = prevByName.get(db.name)
		if (db.tables.length > 0) {
			return {
				...db,
				tables: preserveTablePreviews(cached?.tables, db.tables),
			}
		}
		if (cached && cached.tables.length > 0) {
			return { ...db, tables: cached.tables, meta: cached.meta ?? db.meta }
		}
		return db
	})
}

/** Replace objects for `database` from live; keep other DBs' cached tables. */
export function mergeCatalogReplaceDatabase(
	prev: DatabaseMeta[] | undefined,
	live: DatabaseMeta[],
	database: string,
): DatabaseMeta[] {
	const target = database.trim()
	const prevByName = new Map((prev ?? []).map((d) => [d.name, d]))
	const liveByName = new Map(live.map((d) => [d.name, d]))
	const names = [
		...new Set([
			...live.map((d) => d.name),
			...(prev ?? []).map((d) => d.name),
		]),
	]
	return names.map((name) => {
		const l = liveByName.get(name)
		const p = prevByName.get(name)
		if (target && name === target) {
			if (!l) return { name, tables: [] }
			return {
				...l,
				tables: preserveTablePreviews(p?.tables, l.tables),
			}
		}
		if (l && l.tables.length > 0) {
			return {
				...l,
				tables: preserveTablePreviews(p?.tables, l.tables),
			}
		}
		if (p) return { ...p, meta: l?.meta ?? p.meta }
		return l ?? { name, tables: [] }
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
	sort: TablePreviewSort | null = null,
	codeFilter: string | null = null,
): Promise<TableMeta> {
	const preview = await fetchEngineTablePreview({
		sessionId,
		table: table.name,
		schema: table.schema || null,
		database: table.database || null,
		maxRows,
		offset,
		filters,
		codeFilter,
		sortColumn: sort?.column ?? null,
		sortDir: sort?.dir ?? null,
	})
	return previewToTableMeta(table, preview)
}

/** Background COUNT + FK refs after the page is already on screen. */
export async function loadTablePreviewStats(
	sessionId: string,
	table: TableMeta,
	filters: TablePreviewFilter[] = [],
	codeFilter: string | null = null,
): Promise<TablePreviewStats> {
	return fetchEngineTablePreviewStats({
		sessionId,
		table: table.name,
		schema: table.schema || null,
		database: table.database || null,
		filters,
		codeFilter,
	})
}

export function applyPreviewStats(
	base: TableMeta,
	stats: TablePreviewStats,
): TableMeta {
	const refs = new Map(
		(stats.references ?? [])
			.filter((r) => r.references)
			.map((r) => [r.column, r.references!]),
	)
	const columns =
		refs.size === 0
			? base.columns
			: base.columns.map((c) => {
					const hit = refs.get(c.name)
					if (!hit) return c
					return {
						...c,
						ref: {
							database: hit.database ?? null,
							schema: hit.schema ?? null,
							table: hit.table,
							column: hit.column,
						},
					}
				})
	return {
		...base,
		columns,
		rowCount: stats.totalKnown ? stats.totalRows : base.rowCount,
		totalKnown: stats.totalKnown ? true : base.totalKnown,
	}
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
	const totalKnown = !!preview.totalKnown
	const rowCount = totalKnown ? (preview.totalRows ?? pageCount) : pageCount

	return {
		...base,
		columns: columns.length > 0 ? columns : base.columns,
		rows,
		rowCount,
		totalKnown,
		truncated: !!preview.truncated,
	}
}
