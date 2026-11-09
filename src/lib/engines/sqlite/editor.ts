/** SQLite dialect and completions for the query editor; loaded on demand. */
import { SQLite } from '@codemirror/lang-sql'
import { sqlCatalogCompletion, sqlSchemaFromColumns } from '../sql/complete'
import type { EngineEditorSupport } from '../types'

export const sqliteEditor: EngineEditorSupport = {
	sqlDialect: SQLite,
	completion: (rows) =>
		sqlCatalogCompletion(rows, {
			nestSchemas: false,
			dialect: SQLite,
			kind: 'sqlite',
		}),
	sqlSchema: (rows) =>
		sqlSchemaFromColumns(rows, { nestSchemas: false, quote: 'postgres' }),
}
