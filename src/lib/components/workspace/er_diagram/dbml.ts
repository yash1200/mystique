/**
 * DBML serialize + parse (dbdiagram.io-compatible subset).
 * Docs: https://dbml.dbdiagram.io/docs
 */
import type {
	DataFamily,
	ErColumn,
	ErDiagramState,
	ErEdge,
	ErEdgeKind,
	ErNode,
	ErNodeKind,
} from '$lib/types/er'
import { ER_GRID, snapToGrid } from '$lib/types/er'
import { layoutErNodes } from './erLayout'

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

/** Map DBML type tokens back to designer types (best-effort). */
function designerType(dbml: string, family: DataFamily): string {
	const t = dbml.trim().toLowerCase()
	const sqlMap: Record<string, string> = {
		integer: 'INTEGER',
		int: 'INTEGER',
		bigint: 'BIGINT',
		serial: 'SERIAL',
		uuid: 'UUID',
		text: 'TEXT',
		varchar: 'VARCHAR(255)',
		'varchar(255)': 'VARCHAR(255)',
		boolean: 'BOOLEAN',
		bool: 'BOOLEAN',
		timestamp: 'TIMESTAMP',
		timestamptz: 'TIMESTAMPTZ',
		date: 'DATE',
		numeric: 'NUMERIC',
		decimal: 'NUMERIC',
		jsonb: 'JSONB',
		json: 'JSONB',
		bytea: 'BYTEA',
	}
	if (family === 'sql') return sqlMap[t] ?? dbml.trim().toUpperCase()
	if (family === 'nosql') {
		if (t === 'objectid') return 'ObjectId'
		if (t === 'int' || t === 'integer' || t === 'number') return 'number'
		if (t === 'bool' || t === 'boolean') return 'boolean'
		if (t === 'date' || t === 'timestamp') return 'date'
		return dbml.trim() || 'string'
	}
	// redis
	return dbml.trim() || 'string'
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

	parts.push(
		'// Tip: Sample data (Records { … }) is ignored when generating the diagram.',
	)
	parts.push('')

	return parts.join('\n')
}

// ─── Parse DBML → diagram ───────────────────────────────────────────

export type DbmlParseResult =
	| { ok: true; diagram: ErDiagramState }
	| { ok: false; error: string }

function uid(p: string): string {
	return `${p}_${Math.random().toString(36).slice(2, 9)}`
}

function stripComments(src: string): string {
	// Remove // line comments (not inside quotes — good enough for DBML)
	let s = src.replace(/\/\/[^\n]*/g, '')
	// Remove /* block comments */
	s = s.replace(/\/\*[\s\S]*?\*\//g, '')
	return s
}

function unquoteIdent(raw: string): string {
	const t = raw.trim()
	if (
		(t.startsWith('"') && t.endsWith('"')) ||
		(t.startsWith("'") && t.endsWith("'"))
	) {
		return t.slice(1, -1).replace(/\\"/g, '"')
	}
	return t
}

function nodeKind(family: DataFamily): ErNodeKind {
	if (family === 'nosql') return 'collection'
	if (family === 'redis') return 'key_pattern'
	return 'table'
}

function parseColumnLine(line: string, family: DataFamily): ErColumn | null {
	const trimmed = line.trim()
	if (!trimmed || trimmed.startsWith('//')) return null

	// name type [settings]
	// name type
	const m = trimmed.match(
		/^("(?:\\.|[^"])*"|'(?:\\.|[^'])*'|[A-Za-z_][\w]*)\s+([A-Za-z_][\w()]*(?:\([^)]*\))?)\s*(\[[^\]]*\])?\s*$/,
	)
	if (!m) return null

	const name = unquoteIdent(m[1]!)
	const typeRaw = m[2]!
	const settings = (m[3] ?? '').toLowerCase()
	const pk = settings.includes('primary key') || settings.includes('pk')
	const notNull = settings.includes('not null')

	return {
		id: uid('c'),
		name,
		dataType: designerType(typeRaw, family),
		pk: pk || undefined,
		nullable: notNull ? false : undefined,
	}
}

function extractBlocks(
	src: string,
): { kind: 'table' | 'records'; name: string; body: string }[] {
	const blocks: { kind: 'table' | 'records'; name: string; body: string }[] = []
	// Table name { ... } or Records name(...) { ... } — use brace matching
	const re =
		/\b(Table|table|Records|records)\s+("(?:\\.|[^"])*"|'(?:\\.|[^'])*'|[A-Za-z_][\w]*)\s*(?:\([^)]*\))?\s*\{/g
	let m: RegExpExecArray | null
	while ((m = re.exec(src)) !== null) {
		const kind = m[1]!.toLowerCase() === 'records' ? 'records' : 'table'
		const name = unquoteIdent(m[2]!)
		const startBody = m.index + m[0].length
		let depth = 1
		let i = startBody
		while (i < src.length && depth > 0) {
			const ch = src[i]
			if (ch === '{') depth++
			else if (ch === '}') depth--
			i++
		}
		if (depth !== 0) continue
		const body = src.slice(startBody, i - 1)
		blocks.push({ kind, name, body })
		re.lastIndex = i
	}
	return blocks
}

type RefParsed = {
	leftTable: string
	leftCol: string
	rightTable: string
	rightCol: string
	/** true when left is parent (a < b means b → a) */
	leftIsParent: boolean
	broken: boolean
	label?: string
}

function parseRefs(src: string): RefParsed[] {
	const refs: RefParsed[] = []
	// Ref name: a.b > c.d
	// Ref: a.b > c.d
	// Ref: a.b < c.d
	const re =
		/\bRef(?:\s+([A-Za-z_][\w]*))?\s*:\s*("(?:\\.|[^"])*"|[A-Za-z_][\w]*)\s*\.\s*("(?:\\.|[^"])*"|[A-Za-z_][\w]*)\s*([<>])\s*("(?:\\.|[^"])*"|[A-Za-z_][\w]*)\s*\.\s*("(?:\\.|[^"])*"|[A-Za-z_][\w]*)/gi
	let m: RegExpExecArray | null
	while ((m = re.exec(src)) !== null) {
		const label = m[1]
		const lt = unquoteIdent(m[2]!)
		const lc = unquoteIdent(m[3]!)
		const op = m[4]!
		const rt = unquoteIdent(m[5]!)
		const rc = unquoteIdent(m[6]!)
		const lineEnd = src.slice(m.index, m.index + 200)
		const broken = /broken|optional/i.test(lineEnd)
		refs.push({
			leftTable: lt,
			leftCol: lc,
			rightTable: rt,
			rightCol: rc,
			leftIsParent: op === '<',
			broken,
			label: label || undefined,
		})
	}
	return refs
}

/**
 * Parse DBML text into an ER diagram.
 * Preserves positions of existing tables with the same name when `existing` is set.
 * `Records` blocks are ignored.
 */
export function dbmlToDiagram(
	source: string,
	family: DataFamily,
	existing?: ErDiagramState | null,
): DbmlParseResult {
	const cleaned = stripComments(source ?? '')
	if (!cleaned.trim()) {
		return { ok: false, error: 'DBML is empty — add Table { … } blocks.' }
	}

	const blocks = extractBlocks(cleaned)
	const tableBlocks = blocks.filter((b) => b.kind === 'table')
	if (tableBlocks.length === 0) {
		return {
			ok: false,
			error:
				'No Table blocks found. Example: Table users { id integer [primary key] }',
		}
	}

	const existingByName = new Map(
		(existing?.nodes ?? []).map((n) => [n.name.toLowerCase(), n]),
	)

	const nodes: ErNode[] = []
	const nameToNode = new Map<string, ErNode>()

	tableBlocks.forEach((tb, i) => {
		const cols: ErColumn[] = []
		for (const line of tb.body.split('\n')) {
			const col = parseColumnLine(line, family)
			if (col) cols.push(col)
		}
		// Ensure one pk flag max when multiple marked
		let sawPk = false
		for (const c of cols) {
			if (c.pk) {
				if (sawPk) c.pk = false
				else sawPk = true
			}
		}
		if (cols.length === 0) {
			cols.push({
				id: uid('c'),
				name: family === 'nosql' ? '_id' : 'id',
				dataType:
					family === 'sql'
						? 'UUID'
						: family === 'nosql'
							? 'ObjectId'
							: 'string',
				pk: true,
			})
		}

		const prev = existingByName.get(tb.name.toLowerCase())
		const node: ErNode = {
			id: prev?.id ?? uid('n'),
			kind: nodeKind(family),
			name: tb.name,
			columns: cols,
			x: prev?.x ?? snapToGrid(ER_GRID * 4 + (i % 3) * ER_GRID * 14),
			y: prev?.y ?? snapToGrid(ER_GRID * 4 + Math.floor(i / 3) * ER_GRID * 12),
		}
		nodes.push(node)
		nameToNode.set(tb.name.toLowerCase(), node)
	})

	const edges: ErEdge[] = []
	const refs = parseRefs(cleaned)
	for (const ref of refs) {
		// child.col > parent.col  → from=child, to=parent
		// parent.col < child.col  → from=child, to=parent
		let fromTable: string
		let fromCol: string
		let toTable: string
		let toCol: string
		if (ref.leftIsParent) {
			fromTable = ref.rightTable
			fromCol = ref.rightCol
			toTable = ref.leftTable
			toCol = ref.leftCol
		} else {
			fromTable = ref.leftTable
			fromCol = ref.leftCol
			toTable = ref.rightTable
			toCol = ref.rightCol
		}

		const fromNode = nameToNode.get(fromTable.toLowerCase())
		const toNode = nameToNode.get(toTable.toLowerCase())
		if (!fromNode || !toNode) {
			return {
				ok: false,
				error: `Ref references unknown table: ${fromTable}.${fromCol} → ${toTable}.${toCol}`,
			}
		}
		const fromColumn = fromNode.columns.find(
			(c) => c.name.toLowerCase() === fromCol.toLowerCase(),
		)
		const toColumn = toNode.columns.find(
			(c) => c.name.toLowerCase() === toCol.toLowerCase(),
		)
		if (!fromColumn || !toColumn) {
			return {
				ok: false,
				error: `Ref column not found: ${fromTable}.${fromCol} → ${toTable}.${toCol}`,
			}
		}

		const kind: ErEdgeKind = ref.broken ? 'broken' : 'normal'
		edges.push({
			id: uid('e'),
			from: fromNode.id,
			to: toNode.id,
			fromColumnId: fromColumn.id,
			toColumnId: toColumn.id,
			kind,
			label:
				ref.label ??
				`${fromNode.name}.${fromColumn.name} → ${toNode.name}.${toColumn.name}`,
			waypoints: [],
		})
	}

	return {
		ok: true,
		diagram: {
			family,
			nodes: layoutErNodes(nodes, edges),
			edges,
		},
	}
}
