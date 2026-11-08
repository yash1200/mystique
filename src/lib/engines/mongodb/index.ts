/** MongoDB adapter — mirrors `crates/plugins/mystique-plugin-mongodb`. */
import type { PluginConfig } from '#core/contracts'
import { emptySslDraft, sslPluginFields } from '../ssl'
import type { EngineAdapter } from '../types'
import { mongoCompletionSource } from './complete'
import { formatMql } from './format'
import {
	buildMongoUriFromFields,
	databaseFromMongoUri,
	splitMongoUserinfo,
} from './uri'

export const mongodb: EngineAdapter = {
	id: 'mongodb',
	label: 'MongoDB',
	aliases: ['mongo', 'documentdb'],
	mark: 'Mo',
	description:
		'Document studio — databases, collections, find/aggregation, and collection Model map.',
	available: true,
	defaultPort: 27017,

	connection: {
		uriPlaceholder: 'mongodb://user:pass@127.0.0.1:27017/mydb?authSource=admin',
		emptyDraft: () => ({
			engine: 'mongodb',
			host: '',
			port: 27017,
			// URI is empty; the placeholder shows the expected shape.
			database: 'mystique',
			path: '',
			uri: '',
			username: '',
			password: '',
			...emptySslDraft,
			readOnly: false,
			environment: 'local',
			notes: '',
			queryTimeout: '',
		}),
		toPluginConfig: (draft) => {
			const uri = draft.uri.trim()
			const fromUri = databaseFromMongoUri(uri)
			const database = draft.database.trim() || fromUri || ''
			// Prefer URI when present; otherwise keep discrete fields for legacy profiles.
			const ssl = sslPluginFields(draft)
			if (uri) {
				// Rust stores userinfo in the keychain. The saved URI has no password.
				const creds = splitMongoUserinfo(uri)
				const cfg: PluginConfig = { uri: creds.uri, ...ssl }
				if (database) cfg.database = database
				if (creds.username) cfg.username = creds.username
				if (creds.password) cfg.password = creds.password
				return cfg
			}
			const cfg: PluginConfig = { ...ssl }
			if (draft.host.trim()) cfg.host = draft.host.trim()
			if (draft.port) cfg.port = Number(draft.port) || 27017
			if (draft.username.trim()) cfg.username = draft.username.trim()
			if (database) cfg.database = database
			return cfg
		},
		databaseFromUri: (uri) => databaseFromMongoUri(uri),
		/**
		 * Connector contract: always hand the plugin a `mongodb://` URI, and take
		 * the default database from its path when there is one.
		 */
		sessionPluginConfig: (_profile, merged) => {
			const cfg: PluginConfig = { ...merged }
			let uri = String(cfg.uri ?? '').trim()
			if (!uri) {
				uri = buildMongoUriFromFields(cfg) ?? ''
			}
			if (uri) cfg.uri = uri
			const fromUri = databaseFromMongoUri(uri)
			const storedDb = String(cfg.database ?? '').trim()
			const db = fromUri || storedDb
			if (db) cfg.database = db
			else delete cfg.database
			return cfg
		},
		validate: (draft) =>
			draft.uri.trim() ? null : 'Connection URI is required.',
	},

	language: {
		id: 'mql',
		format: formatMql,
		completion: (rows) => mongoCompletionSource(rows),
	},

	defaultCapabilities: {
		engine: 'mongodb',
		family: 'DOCUMENT',
		queryLanguage: 'MQL',
		sqlDialect: '',
		supportsSchema: false,
		supportsForeignKeys: false,
		supportsErDiagram: false,
		supportsModelView: true,
		modelViewKind: 'COLLECTION_MAP',
		supportsPagination: true,
		supportsWrite: false,
		supportsCancelQuery: true,
		supportsTransactions: false,
		supportsExplainPlan: true,
		supportsStructureEdit: true,
		supportsQuery: true,
		supportsPooling: true,
		bindsSessionToDatabase: false,
		preferDatabaseFromUri: true,
		connectionKind: 'uri',
		systemDatabases: ['admin', 'local', 'config'],
		nameExample: 'e.g. local-mongo',
		objectLabels: {
			namespace: 'Database',
			container: 'Collection',
			field: 'Field',
		},
		explorer: {
			layout: 'flat',
			multiDatabase: true,
			canCreateDatabase: true,
			canCreateGroup: false,
			canCreateObject: true,
			databaseLabel: 'Database',
			groupLabel: '',
			objectLabel: 'collections',
			objectLabelSingular: 'collection',
			listTitle: 'Collections',
			filterPlaceholder: 'Filter collections…',
		},
		codeFilterPlaceholder: '{ "email": "user@example.com" }',
		tableAdmin: {
			createStatement: true,
			rename: true,
			drop: true,
			truncate: true,
			duplicate: true,
			viewStructure: true,
		},
		exportFormats: ['csv', 'json', 'jsonl'],
		supportsServerMetrics: true,
		supportsTerminateSession: true,
		supportsExplorer: true,
		supportDepth: 'operate',
	},
}
