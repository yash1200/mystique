/**
 * Plugin-owned engine config (`EngineCapabilities`, generated from connector-api).
 * The FE must render chrome from this — not from `if engine === "mongodb"`.
 */
export type {
	ConnectionKind,
	EngineCapabilities,
	EngineFamily,
	ExplorerConfig,
	ModelViewKind,
	ObjectLabels,
	SupportDepth,
	TableAdminFeatures,
} from '../ipc/bindings.gen'

/** High-level data family for live ER / model views. */
export type DataFamily = 'sql' | 'nosql' | 'redis'

export type ExplorerLayout = 'schema-tree' | 'flat'
