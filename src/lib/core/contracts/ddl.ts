/** Structure editor and same-engine schema diff. Mirrors `mystique-connector-api` ddl. */

export interface ColumnOriginal {
	name: string
	dataType: string
	nullable: boolean
	pkPosition?: number
	/** MySQL column definition from the server. Sent back unchanged. */
	definition?: string | null
}

export interface ColumnDraft {
	name: string
	dataType: string
	nullable: boolean
	pkPosition?: number
	original?: ColumnOriginal | null
	drop?: boolean
}

export interface IndexOriginal {
	name: string
	columns: string[]
	unique: boolean
	primary?: boolean
}

export interface IndexDraft {
	name: string
	columns: string[]
	unique: boolean
	primary?: boolean
	concurrent?: boolean
	original?: IndexOriginal | null
	drop?: boolean
}

export interface ForeignKeyOriginal {
	name: string
	columns: string[]
	refSchema?: string | null
	refTable: string
	refColumns: string[]
}

export interface ForeignKeyDraft {
	name: string
	columns: string[]
	refSchema?: string | null
	refTable: string
	refColumns: string[]
	original?: ForeignKeyOriginal | null
	drop?: boolean
}

export interface TableDefinition {
	database?: string | null
	schema?: string | null
	name: string
	kind?: string
	columns?: ColumnDraft[]
	indexes?: IndexDraft[]
	foreignKeys?: ForeignKeyDraft[]
	schemaRevision?: string | null
	validator?: string | null
	originalValidator?: string | null
	create?: boolean
	dropTable?: boolean
	renameTo?: string | null
}

export interface DdlPlan {
	statements: string[]
	warnings: string[]
	transactional: boolean
	requiresRebuild: boolean
	note?: string | null
}

export interface ApplyDdlResult {
	applied: string[]
	message: string
}

export interface RenamePair {
	from: string
	to: string
}

export interface SchemaChange {
	name: string
	action: string
	plan: DdlPlan
	draft: TableDefinition
}

export interface SchemaMigration {
	changes: SchemaChange[]
	warnings: string[]
}

export interface DescribeTableDefinitionRequest {
	sessionId: string
	table: string
	schema?: string | null
	database?: string | null
	kind?: string | null
}

export interface ListTableDefinitionsRequest {
	sessionId: string
	database?: string | null
	schema?: string | null
}

export interface PlanDdlRequest {
	sessionId: string
	draft: TableDefinition
}

export interface ApplyDdlRequest {
	sessionId: string
	draft: TableDefinition
	confirmObjectName?: string | null
}

export interface PlanSchemaDiffRequest {
	sessionId: string
	source: TableDefinition[]
	target: TableDefinition[]
	renames?: RenamePair[]
}

export interface ApplySchemaDiffRequest {
	sessionId: string
	source: TableDefinition[]
	target: TableDefinition[]
	renames?: RenamePair[]
	confirmObjectName?: string | null
}
