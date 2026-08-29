/** MySQL / MariaDB adapter — mirrors `crates/plugins/mystique-plugin-mysql`. */
import { MySQL } from '@codemirror/lang-sql'
import { relationalCapabilities } from '../capabilities'
import { sqlSchemaFromColumns } from '../sql/complete'
import { sqlFormatter } from '../sql/format'
import type { EngineAdapter } from '../types'
import Fields from './Fields.svelte'

export const mysql: EngineAdapter = {
	id: 'mysql',
	label: 'MySQL',
	aliases: ['mariadb'],
	mark: 'My',
	description:
		'MySQL / MariaDB — databases, tables, SQL query space, and relational Model view.',
	available: true,
	defaultPort: 3306,

	connection: {
		Fields,
		emptyDraft: () => ({
			engine: 'mysql',
			host: '127.0.0.1',
			port: 3306,
			// Docker seed: admin/123, database mystique (see docs/DEV_MYSQL.md).
			database: 'mystique',
			path: '',
			uri: '',
			username: 'admin',
			password: '123',
			ssl: false,
			readOnly: false,
			environment: 'local',
			notes: '',
		}),
		toPluginConfig: (draft) => ({
			host: draft.host.trim() || '127.0.0.1',
			port: Number(draft.port) || 3306,
			database: draft.database.trim() || 'mystique',
			username: draft.username.trim() || 'admin',
			ssl: draft.ssl,
			sslMode: draft.ssl ? 'require' : 'disable',
		}),
		validate: (draft, mode) => {
			if (!draft.host.trim() || !draft.database.trim()) {
				return 'Host and database are required.'
			}
			if (mode === 'test' && !draft.username.trim()) {
				return 'Username is required (Docker seed: admin).'
			}
			return null
		},
	},

	language: {
		id: 'sql',
		format: sqlFormatter('mysql'),
		sqlDialect: MySQL,
		completion: () => null,
		// MySQL databases are not schemas — keep the table list flat.
		sqlSchema: (rows) => sqlSchemaFromColumns(rows, { nestSchemas: false }),
	},

	defaultCapabilities: relationalCapabilities({
		engine: 'mysql',
		sqlDialect: 'mysql',
		nameExample: 'e.g. local-mysql',
	}),
}
