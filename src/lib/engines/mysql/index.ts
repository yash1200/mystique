/** MySQL / MariaDB adapter — mirrors `crates/plugins/mystique-plugin-mysql`. */
import { relationalCapabilities } from '../capabilities'
import { emptySslDraft, sslPluginFields } from '../ssl'
import { sqlFormatter } from '../sql/format'
import type { EngineAdapter } from '../types'

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
			...emptySslDraft,
			readOnly: false,
			environment: 'local',
			notes: '',
			queryTimeout: '',
			savePassword: true,
		}),
		toPluginConfig: (draft) => ({
			host: draft.host.trim() || '127.0.0.1',
			port: Number(draft.port) || 3306,
			database: draft.database.trim() || 'mystique',
			username: draft.username.trim() || 'admin',
			...sslPluginFields(draft),
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
		editor: () => import('./editor').then((m) => m.mysqlEditor),
	},

	defaultCapabilities: relationalCapabilities({
		engine: 'mysql',
		supportsExplainPlan: true,
		sqlDialect: 'mysql',
		nameExample: 'e.g. local-mysql',
	}),
}
