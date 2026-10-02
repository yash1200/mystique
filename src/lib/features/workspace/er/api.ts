/**
 * Build a live ER / model diagram from plugin catalog + relationships.
 * Layout stays in the FE; SQL / FK discovery lives in engine plugins.
 */
import type {
	CatalogColumnRow,
	CatalogRelationship,
	DataFamily,
	EngineCapabilities,
} from '#core/contracts'
import { listEngineRelationships } from '#core/ipc'
import { familyFromCaps, modelTabTitle, resolveCapabilities } from '#engines'
import { catalogColumnsCache } from '#features/workspace/catalog'
import { layoutErNodes } from './layout'
import type {
	ErColumn,
	ErDiagramState,
	ErEdge,
	ErNode,
	LiveErFocus,
	LiveErLoadResult,
} from './types'
function nodeId(schema: string, name: string): string {
	const s = schema?.trim()
	return s ? `n_${s}__${name}` : `n_${name}`
}

function colId(schema: string, table: string, column: string): string {
	const s = schema?.trim()
	return s ? `c_${s}__${table}__${column}` : `c_${table}__${column}`
}

export type { LiveErFocus, LiveErLoadResult }

export function erNodeIdFor(
	schema: string | null | undefined,
	name: string,
): string {
	return nodeId((schema ?? '').trim(), name)
}

export async function loadLiveErDiagram(
	sessionId: string,
	engine: string,
	database: string | null | undefined,
	caps?: EngineCapabilities | null,
	focus?: LiveErFocus | null,
): Promise<LiveErLoadResult> {
	if (!focus?.name?.trim()) {
		throw new Error('Pick a table to open its ER diagram.')
	}

	const resolved = resolveCapabilities(engine, caps)
	const family = familyFromCaps(resolved)
	const db = (database ?? '').trim() || null
	const catalogCols = await catalogColumnsCache.load(sessionId, db)
	const rels = await listEngineRelationships({
		sessionId,
		database: db,
	})
	const defaultSchema = db
	const keep = neighborhoodKeys(focus, rels, defaultSchema)
	const focusedCols = catalogCols.filter((row) => {
		if (!row.table) return false
		const schema = row.schema?.trim() || defaultSchema || ''
		return keep.has(tableKey(schema, row.table))
	})
	const focusedRels = rels.filter((rel) => {
		const fromSchema = rel.fromSchema?.trim() || defaultSchema || ''
		const toSchema = rel.toSchema?.trim() || defaultSchema || ''
		return (
			keep.has(tableKey(fromSchema, rel.fromTable)) &&
			keep.has(tableKey(toSchema, rel.toTable))
		)
	})

	const diagram = buildFromCatalogColumns(family, focusedCols, defaultSchema)
	ensureFocusNode(diagram, family, focus, defaultSchema)
	addRelationshipEdges(diagram, focusedRels, defaultSchema)

	const focusNodeId = erNodeIdFor(
		(focus.schema ?? '').trim() || defaultSchema,
		focus.name,
	)
	const title = modelTabTitle(resolved, db, focus.name)
	const noun =
		resolved.explorer.objectLabel ||
		(family === 'nosql'
			? 'collections'
			: family === 'redis'
				? 'key patterns'
				: 'tables')
	const relLabel = resolved.supportsForeignKeys
		? `${diagram.edges.length} relationships`
		: diagram.edges.length > 0
			? `${diagram.edges.length} inferred links`
			: 'no declared FKs'
	return {
		diagram: {
			...diagram,
			nodes: layoutErNodes(diagram.nodes, diagram.edges),
		},
		title,
		summary: `${diagram.nodes.length} ${noun} · ${relLabel}`,
		focusNodeId,
	}
}

function tableKey(schema: string, name: string): string {
	return `${schema}::${name}`
}

function neighborhoodKeys(
	focus: LiveErFocus,
	rels: CatalogRelationship[],
	defaultSchema: string | null,
): Set<string> {
	const focusSchema = (focus.schema ?? '').trim() || defaultSchema || ''
	const focusName = focus.name.trim()
	const keep = new Set<string>([tableKey(focusSchema, focusName)])
	for (const rel of rels) {
		const fromSchema = rel.fromSchema?.trim() || defaultSchema || ''
		const toSchema = rel.toSchema?.trim() || defaultSchema || ''
		const fromKey = tableKey(fromSchema, rel.fromTable)
		const toKey = tableKey(toSchema, rel.toTable)
		if (fromKey === tableKey(focusSchema, focusName)) keep.add(toKey)
		if (toKey === tableKey(focusSchema, focusName)) keep.add(fromKey)
	}
	return keep
}

function ensureFocusNode(
	diagram: ErDiagramState,
	family: DataFamily,
	focus: LiveErFocus,
	defaultSchema: string | null,
) {
	const schema = (focus.schema ?? '').trim() || defaultSchema || ''
	const id = nodeId(schema, focus.name)
	if (diagram.nodes.some((n) => n.id === id)) return
	diagram.nodes.push({
		id,
		kind: family === 'nosql' ? 'collection' : 'table',
		name:
			schema && schema !== 'public' && schema !== 'main'
				? `${schema}.${focus.name}`
				: focus.name,
		columns: [],
		x: 0,
		y: 0,
	})
}

function buildFromCatalogColumns(
	family: DataFamily,
	rows: CatalogColumnRow[],
	defaultSchema: string | null,
): ErDiagramState {
	type Acc = { schema: string; name: string; columns: ErColumn[] }
	const tables = new Map<string, Acc>()

	for (const row of rows) {
		const table = row.table
		const column = row.column
		if (!table || !column) continue
		const schema = row.schema?.trim() || defaultSchema || ''
		const key = `${schema}::${table}`
		let acc = tables.get(key)
		if (!acc) {
			acc = { schema, name: table, columns: [] }
			tables.set(key, acc)
		}
		acc.columns.push({
			id: colId(schema, table, column),
			name: column,
			dataType: row.dataType || 'unknown',
			pk: !!row.pk,
			nullable: row.nullable,
		})
	}

	const nodes: ErNode[] = [...tables.values()].map((t) => ({
		id: nodeId(t.schema, t.name),
		kind: family === 'nosql' ? 'collection' : 'table',
		name:
			t.schema && t.schema !== 'public' && t.schema !== 'main'
				? `${t.schema}.${t.name}`
				: t.name,
		columns: t.columns,
		x: 0,
		y: 0,
	}))

	return { family, nodes, edges: [] }
}

function addRelationshipEdges(
	diagram: ErDiagramState,
	rels: CatalogRelationship[],
	defaultSchema: string | null = null,
) {
	const nodeIds = new Set(diagram.nodes.map((n) => n.id))
	const edges: ErEdge[] = []
	let i = 0
	for (const rel of rels) {
		const fromSchema = rel.fromSchema?.trim() || defaultSchema || ''
		const toSchema = rel.toSchema?.trim() || defaultSchema || ''
		const fromId = nodeId(fromSchema, rel.fromTable)
		const toId = nodeId(toSchema, rel.toTable)
		if (!nodeIds.has(fromId) || !nodeIds.has(toId)) continue
		edges.push({
			id: `e_${i++}_${fromId}_${toId}`,
			from: fromId,
			to: toId,
			fromColumnId: colId(fromSchema, rel.fromTable, rel.fromColumn),
			toColumnId: colId(toSchema, rel.toTable, rel.toColumn),
			kind: 'fk',
		})
	}
	diagram.edges = edges
}
