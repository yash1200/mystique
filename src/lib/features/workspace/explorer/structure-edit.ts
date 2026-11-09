/**
 * Draft edits for the structure tab. SQL stays on the server.
 */
import type {
	ColumnDraft,
	DiffSelection,
	DropKey,
	ForeignKeyDraft,
	IndexDraft,
	RenamePair,
	SchemaChange,
	TableDefinition,
} from '../../../core/contracts/ddl'

export function draftFingerprint(draft: TableDefinition): string {
	return JSON.stringify(draft)
}

/** Plain copy of the draft to plan or apply, with the collection validator text. */
export function draftToSend(draft: TableDefinition, validatorText: string): TableDefinition {
	const copy = JSON.parse(draftFingerprint(draft)) as TableDefinition
	if ((copy.kind ?? '').toLowerCase() === 'collection') {
		copy.validator = validatorText.trim() ? validatorText : null
	}
	return copy
}

export function columnListText(columns: string[] | undefined): string {
	return (columns ?? []).join(', ')
}

function columnList(text: string): string[] {
	return text
		.split(',')
		.map((part) => part.trim())
		.filter((part) => part.length > 0)
}

export function addColumn(draft: TableDefinition): TableDefinition {
	const column: ColumnDraft = {
		name: '',
		dataType: 'text',
		nullable: true,
		pkPosition: 0,
		drop: false,
	}
	return { ...draft, columns: [...(draft.columns ?? []), column] }
}

export function addIndex(draft: TableDefinition): TableDefinition {
	const index: IndexDraft = {
		name: '',
		columns: [],
		unique: false,
		primary: false,
		concurrent: false,
		drop: false,
	}
	return { ...draft, indexes: [...(draft.indexes ?? []), index] }
}

export function addForeignKey(draft: TableDefinition): TableDefinition {
	const fk: ForeignKeyDraft = {
		name: '',
		columns: [],
		refSchema: null,
		refTable: '',
		refColumns: [],
		drop: false,
	}
	return { ...draft, foreignKeys: [...(draft.foreignKeys ?? []), fk] }
}

export function dropColumn(draft: TableDefinition, index: number): TableDefinition {
	return {
		...draft,
		columns: (draft.columns ?? []).flatMap((column, i) => {
			if (i !== index) return [column]
			if (!column.original) return []
			return [{ ...column, drop: true }]
		}),
	}
}

export function restoreColumn(draft: TableDefinition, index: number): TableDefinition {
	return {
		...draft,
		columns: (draft.columns ?? []).map((column, i) =>
			i === index ? { ...column, drop: false } : column,
		),
	}
}

export function dropIndex(draft: TableDefinition, index: number): TableDefinition {
	return {
		...draft,
		indexes: (draft.indexes ?? []).flatMap((item, i) => {
			if (i !== index) return [item]
			if (!item.original) return []
			return [{ ...item, drop: true }]
		}),
	}
}

export function restoreIndex(draft: TableDefinition, index: number): TableDefinition {
	return {
		...draft,
		indexes: (draft.indexes ?? []).map((item, i) =>
			i === index ? { ...item, drop: false } : item,
		),
	}
}

export function dropForeignKey(draft: TableDefinition, index: number): TableDefinition {
	return {
		...draft,
		foreignKeys: (draft.foreignKeys ?? []).flatMap((fk, i) => {
			if (i !== index) return [fk]
			if (!fk.original) return []
			return [{ ...fk, drop: true }]
		}),
	}
}

export function restoreForeignKey(
	draft: TableDefinition,
	index: number,
): TableDefinition {
	return {
		...draft,
		foreignKeys: (draft.foreignKeys ?? []).map((fk, i) =>
			i === index ? { ...fk, drop: false } : fk,
		),
	}
}

export function setIndexColumns(
	draft: TableDefinition,
	index: number,
	text: string,
): TableDefinition {
	const columns = columnList(text)
	return {
		...draft,
		indexes: (draft.indexes ?? []).map((item, i) =>
			i === index ? { ...item, columns } : item,
		),
	}
}

export function setForeignKeyColumns(
	draft: TableDefinition,
	index: number,
	text: string,
	side: 'columns' | 'refColumns',
): TableDefinition {
	const columns = columnList(text)
	return {
		...draft,
		foreignKeys: (draft.foreignKeys ?? []).map((fk, i) =>
			i === index ? { ...fk, [side]: columns } : fk,
		),
	}
}

export function unpairedNames(
	source: string[],
	target: string[],
	renames: RenamePair[],
): { sourceOnly: string[]; targetOnly: string[] } {
	const tables = tablePairs(renames)
	const renamedFrom = new Set(tables.map((pair) => pair.from))
	const renamedTo = new Set(tables.map((pair) => pair.to))
	const targetNames = new Set(target)
	const sourceNames = new Set(source)
	return {
		sourceOnly: source.filter((name) => !targetNames.has(name) && !renamedFrom.has(name)),
		targetOnly: target.filter((name) => !sourceNames.has(name) && !renamedTo.has(name)),
	}
}

/** A pair counts only when the user chose both names. */
export function pairRename(
	renames: RenamePair[],
	from: string,
	to: string,
): RenamePair[] {
	const left = from.trim()
	const right = to.trim()
	if (!left || !right) return renames
	if (tablePairs(renames).some((pair) => pair.from === left || pair.to === right)) return renames
	return [...renames, { from: left, to: right }]
}

/** Table pairs only (column pairs carry a `table`). */
export function tablePairs(renames: RenamePair[]): RenamePair[] {
	return renames.filter((pair) => !pair.table)
}

/** Source-side table name of a diff change (the name it ends with). */
export function sourceTableOf(change: SchemaChange): string {
	return change.draft.renameTo?.trim() || change.name
}

/** Pair two columns as a rename, replacing any pair either side was in. */
export function pairColumns(
	renames: RenamePair[],
	table: string,
	from: string,
	to: string,
): RenamePair[] {
	const left = from.trim()
	const right = to.trim()
	if (!left || !right) return renames
	const rest = renames.filter(
		(pair) => pair.table !== table || (pair.from !== left && pair.to !== right),
	)
	return [...rest, { from: left, to: right, table, keepApart: false }]
}

/** The user says a guessed rename is not one: keep the columns apart. */
export function keepColumnsApart(
	renames: RenamePair[],
	table: string,
	from: string,
	to: string,
): RenamePair[] {
	const rest = renames.filter(
		(pair) => !(pair.table === table && pair.from === from && pair.to === to),
	)
	return [...rest, { from, to, table, keepApart: true }]
}

/** Remove an explicit column pair or a "not a rename" mark. */
export function unpairColumns(
	renames: RenamePair[],
	table: string,
	from: string,
	to: string,
): RenamePair[] {
	return renames.filter(
		(pair) => !(pair.table === table && pair.from === from && pair.to === to),
	)
}

/** Columns the diff adds to a table (source-only, not renamed). */
export function addedColumns(change: SchemaChange): string[] {
	if (change.action !== 'alter' && change.action !== 'unchanged') return []
	return (change.draft.columns ?? [])
		.filter((column) => !column.original && !column.drop)
		.map((column) => column.name)
}

export function emptySelection(): DiffSelection {
	return { skip: [], drops: [] }
}

function sameDrop(a: DropKey, b: DropKey): boolean {
	return a.table === b.table && a.kind === b.kind && a.name === b.name
}

/** Include or leave out one change (a table drop is a drop key). */
export function setChangeSelected(
	selection: DiffSelection,
	change: SchemaChange,
	on: boolean,
): DiffSelection {
	if (change.action === 'drop') {
		return setDropIncluded(
			selection,
			{ table: change.name, kind: 'table', name: change.name },
			on,
		)
	}
	const skip = (selection.skip ?? []).filter((name) => name !== change.name)
	return { ...selection, skip: on ? skip : [...skip, change.name] }
}

/** Include or leave out one drop. Drops are off until chosen. */
export function setDropIncluded(
	selection: DiffSelection,
	key: DropKey,
	on: boolean,
): DiffSelection {
	const drops = (selection.drops ?? []).filter((item) => !sameDrop(item, key))
	return { ...selection, drops: on ? [...drops, key] : drops }
}

export function isCollection(draft: TableDefinition): boolean {
	return (draft.kind ?? '').toLowerCase() === 'collection'
}
