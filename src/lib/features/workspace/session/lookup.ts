/** Catalog lookups shared by the shell and the panes (no store coupling). */

import type {
	CatalogObjectKind,
	DatabaseMeta,
	TableMeta,
} from '#core/contracts'

export function catalogsFor(
	catalogs: Record<string, DatabaseMeta[]>,
	connectionId: string | null | undefined,
): DatabaseMeta[] {
	if (!connectionId) return []
	return catalogs[connectionId] ?? []
}

export function findCatalogTable(
	catalogs: DatabaseMeta[],
	tableId: string | null | undefined,
): TableMeta | null {
	if (!tableId) return null
	for (const db of catalogs) {
		const hit = db.tables.find((t) => t.id === tableId)
		if (hit) return hit
	}
	return null
}

export function objectKindById(
	catalogs: DatabaseMeta[],
): Record<string, CatalogObjectKind> {
	const map: Record<string, CatalogObjectKind> = {}
	for (const db of catalogs) {
		for (const t of db.tables) map[t.id] = t.kind
	}
	return map
}

export function selectedDatabaseName(
	selectedDb: Record<string, string>,
	connectionId: string | null | undefined,
	catalogs: DatabaseMeta[],
): string {
	if (!connectionId) return ''
	return selectedDb[connectionId] ?? catalogs[0]?.name ?? ''
}

export function patchCatalogTable(
	catalogs: Record<string, DatabaseMeta[]>,
	connectionId: string,
	tableId: string,
	next: TableMeta,
): Record<string, DatabaseMeta[]> {
	const list = catalogs[connectionId] ?? []
	return {
		...catalogs,
		[connectionId]: list.map((db) => ({
			...db,
			tables: db.tables.map((t) => (t.id === tableId ? next : t)),
		})),
	}
}
