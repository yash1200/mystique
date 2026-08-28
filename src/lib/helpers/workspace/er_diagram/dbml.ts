/**
 * DBML serialize (dbdiagram.io-compatible subset).
 * Docs: https://dbml.dbdiagram.io/docs
 */
import type {
	DataFamily,
	ErDiagramState,
	ErEdge,
	ErNode,
} from '$lib/types/er'

function quoteIdent(name: string): string {
	const n = name.trim() || 'unnamed'
	if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(n)) return n
	return `"${n.replace(/"/g, '\\"')}"`
}

function dbmlType(dataType: string): string {
	const t = (dataType || 'text').trim()
	const map: Record<string, string> = {
		INTEGER: 'integer',
		BIGINT: 'bigint',
		SERIAL: 'integer',
		UUID: 'uuid',
		TEXT: 'text',
		'VARCHAR(255)': 'varchar',
		BOOLEAN: 'boolean',
		TIMESTAMP: 'timestamp',
		TIMESTAMPTZ: 'timestamptz',
		DATE: 'date',
		NUMERIC: 'numeric',
		JSONB: 'jsonb',
		BYTEA: 'bytea',
		ObjectId: 'ObjectId',
	}
	return map[t] ?? t.toLowerCase()
}

function keyword(_family: DataFamily): string {
	return 'Table'
}

function headerComment(family: DataFamily, diagramName: string): string {
	const lines = [
		'// Use DBML to define your database structure',
		'// Docs: https://dbml.dbdiagram.io/docs',
		`// Generated from diagram: ${diagramName || 'Untitled'}`,
	]
	if (family === 'nosql') {
		lines.push(
			'// Family: NoSQL — Table blocks map to collections; fields are document fields.',
		)
	} else if (family === 'redis') {
		lines.push(
			'// Family: Redis — Table blocks map to key patterns; fields are attributes.',
		)
	} else {
		lines.push('// Family: SQL')
	}
	lines.push('')
	return lines.join('\n')
}

function emitTable(node: ErNode, family: DataFamily): string {
	const kw = keyword(family)
	const lines: string[] = [`${kw} ${quoteIdent(node.name)} {`]
	for (const col of node.columns) {
		const settings: string[] = []
		if (col.pk) settings.push('primary key')
		if (col.nullable === false) settings.push('not null')
		const settingStr = settings.length ? ` [${settings.join(', ')}]` : ''
		lines.push(
			`  ${quoteIdent(col.name)} ${dbmlType(col.dataType)}${settingStr}`,
		)
	}
	if (node.columns.length === 0) {
		lines.push('  // (no columns yet)')
	}
	lines.push('}')
	return lines.join('\n')
}

function resolveColName(node: ErNode | undefined, colId: string): string {
	if (!node) return colId
	return node.columns.find((c) => c.id === colId)?.name ?? colId
}

function emitRef(
	edge: ErEdge,
	nodesById: Map<string, ErNode>,
	index: number,
): string {
	const fromNode = nodesById.get(edge.from)
	const toNode = nodesById.get(edge.to)
	const fromTable = quoteIdent(fromNode?.name ?? edge.from)
	const toTable = quoteIdent(toNode?.name ?? edge.to)
	const fromCol = quoteIdent(resolveColName(fromNode, edge.fromColumnId))
	const toCol = quoteIdent(resolveColName(toNode, edge.toColumnId))

	const isBroken =
		edge.kind === 'broken' ||
		edge.kind === 'inferred' ||
		edge.kind === 'logical'

	const name =
		edge.label &&
		/^[A-Za-z_][A-Za-z0-9_]*$/.test(edge.label.replace(/\s+/g, '_'))
			? edge.label.replace(/\W+/g, '_').replace(/^_+|_+$/g, '') ||
				`rel_${index + 1}`
			: `rel_${index + 1}`

	if (isBroken) {
		return `Ref ${name}: ${fromTable}.${fromCol} > ${toTable}.${toCol} // optional / broken`
	}
	return `Ref: ${fromTable}.${fromCol} > ${toTable}.${toCol}`
}

/**
 * Convert current diagram to live DBML text (tables + refs).
 */
export function diagramToDbml(
	diagram: ErDiagramState | null | undefined,
	diagramName = 'Untitled diagram',
): string {
	if (!diagram) {
		return [
			headerComment('sql', diagramName).trimEnd(),
			'',
			'// Add tables on the canvas to generate DBML.',
			'',
		].join('\n')
	}

	const parts: string[] = [headerComment(diagram.family, diagramName)]

	for (const node of diagram.nodes) {
		parts.push(emitTable(node, diagram.family))
		parts.push('')
	}

	if (diagram.edges.length > 0) {
		parts.push('// Relationships')
		const byId = new Map(diagram.nodes.map((n) => [n.id, n]))
		diagram.edges.forEach((edge, i) => {
			parts.push(emitRef(edge, byId, i))
		})
		parts.push('')
	}

	return parts.join('\n')
}
