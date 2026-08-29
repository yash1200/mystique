/** SQLite adapter — mirrors `crates/plugins/mystique-plugin-sqlite`. */
import { SQLite } from '@codemirror/lang-sql'
import { relationalCapabilities } from '../capabilities'
import { sqlSchemaFromColumns } from '../sql/complete'
import { sqlFormatter } from '../sql/format'
import type { EngineAdapter } from '../types'
import Fields from './Fields.svelte'

export const sqlite: EngineAdapter = {
	id: 'sqlite',
	label: 'SQLite',
	mark: 'SQL',
	description:
		'Local SQL file — same relational workspace as Postgres for offline and embedded DBs.',
	available: true,
	defaultPort: 0,

	connection: {
		Fields,
		emptyDraft: () => ({
			engine: 'sqlite',
			host: '',
			port: 0,
			database: '',
			path: '',
			uri: '',
			username: '',
			password: '',
			ssl: false,
			readOnly: false,
			environment: 'local',
			notes: '',
		}),
		toPluginConfig: (draft) => ({
			path: draft.path.trim() || draft.database.trim(),
		}),
		validate: (draft) =>
			draft.path.trim() ? null : 'SQLite database file path is required.',
	},

	language: {
		id: 'sql',
		format: sqlFormatter('sqlite'),
		sqlDialect: SQLite,
		completion: () => null,
		sqlSchema: (rows) => sqlSchemaFromColumns(rows, { nestSchemas: false }),
	},

	defaultCapabilities: relationalCapabilities({
		engine: 'sqlite',
		sqlDialect: 'sqlite',
		supportsCancelQuery: false,
		bindsSessionToDatabase: false,
		connectionKind: 'file',
		nameExample: 'e.g. app-dev.sqlite',
		explorer: {
			layout: 'flat',
			multiDatabase: false,
			canCreateDatabase: false,
			canCreateGroup: false,
			canCreateObject: true,
			databaseLabel: 'Database',
			groupLabel: '',
			objectLabel: 'tables',
			objectLabelSingular: 'table',
			listTitle: 'Tables',
			filterPlaceholder: 'Filter tables…',
		},
	}),
}
