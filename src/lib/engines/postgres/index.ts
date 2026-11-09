/** PostgreSQL adapter — mirrors `crates/plugins/mystique-plugin-postgres`. */
import { relationalCapabilities } from '../capabilities'
import { emptySslDraft, sslPluginFields } from '../ssl'
import { sqlFormatter } from '../sql/format'
import type { EngineAdapter } from '../types'

export const postgres: EngineAdapter = {
	id: 'postgres',
	label: 'PostgreSQL',
	aliases: ['postgresql', 'pg'],
	mark: 'Pg',
	description:
		'Relational studio — schemas, tables, SQL, and classic ER Model view from foreign keys.',
	available: true,
	defaultPort: 5432,

	connection: {
		emptyDraft: () => ({
			engine: 'postgres',
			host: '127.0.0.1',
			port: 5432,
			// Docker images use db/user `postgres`.
			database: 'postgres',
			path: '',
			uri: '',
			username: 'postgres',
			// Docker seed: user/password postgres (Host placeholder matches).
			password: 'postgres',
			...emptySslDraft,
			readOnly: false,
			environment: 'local',
			notes: '',
			queryTimeout: '',
			savePassword: true,
		}),
		toPluginConfig: (draft) => ({
			host: draft.host.trim() || '127.0.0.1',
			port: Number(draft.port) || 5432,
			database: draft.database.trim() || 'postgres',
			// Prefer explicit user; empty string is omitted so backend default applies
			username: draft.username.trim() || 'postgres',
			...sslPluginFields(draft),
		}),
		validate: (draft, mode) => {
			if (!draft.host.trim() || !draft.database.trim()) {
				return 'Host and database are required.'
			}
			if (mode === 'test' && !draft.username.trim()) {
				return 'Username is required (Docker: postgres · Homebrew: your macOS login).'
			}
			return null
		},
	},

	language: {
		id: 'sql',
		format: sqlFormatter('postgresql'),
		editor: () => import('./editor').then((m) => m.postgresEditor),
	},

	defaultCapabilities: relationalCapabilities({
		engine: 'postgres',
		supportsExplainPlan: true,
		sqlDialect: 'postgresql',
		nameExample: 'e.g. local-pg',
		explorer: {
			layout: 'schema-tree',
			multiDatabase: true,
			canCreateDatabase: true,
			canCreateGroup: true,
			canCreateObject: true,
			databaseLabel: 'Database',
			groupLabel: 'Schema',
			objectLabel: 'tables',
			objectLabelSingular: 'table',
			listTitle: 'Tables',
			filterPlaceholder: 'Filter schemas & tables…',
		},
	}),
}
