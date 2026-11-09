/** PostgreSQL dialect and completions for the query editor; loaded on demand. */
import { PostgreSQL } from '@codemirror/lang-sql'
import { sqlCatalogCompletion, sqlSchemaFromColumns } from '../sql/complete'
import type { EngineEditorSupport } from '../types'

export const postgresEditor: EngineEditorSupport = {
	sqlDialect: PostgreSQL,
	completion: (rows) =>
		sqlCatalogCompletion(rows, {
			nestSchemas: true,
			dialect: PostgreSQL,
			kind: 'postgresql',
		}),
	// Postgres always nests by schema, even with a single non-public schema.
	sqlSchema: (rows) =>
		sqlSchemaFromColumns(rows, { nestSchemas: true, quote: 'postgres' }),
}
