/** MySQL / MariaDB dialect and completions for the query editor; loaded on demand. */
import { MySQL } from '@codemirror/lang-sql'
import { sqlCatalogCompletion, sqlSchemaFromColumns } from '../sql/complete'
import type { EngineEditorSupport } from '../types'

export const mysqlEditor: EngineEditorSupport = {
	sqlDialect: MySQL,
	completion: (rows) =>
		sqlCatalogCompletion(rows, {
			nestSchemas: false,
			dialect: MySQL,
			kind: 'mysql',
		}),
	// MySQL databases are not schemas — keep the table list flat.
	sqlSchema: (rows) =>
		sqlSchemaFromColumns(rows, { nestSchemas: false, quote: 'mysql' }),
}
