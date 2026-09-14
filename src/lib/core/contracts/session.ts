import type { ConnectionProfile } from './connection'
import type { EngineCapabilities } from './engine-caps'

/** Live engine-pool health for an open workspace session. */
export type SessionLiveStatus =
	| 'connected'
	| 'disconnected'
	| 'reconnecting'
	| 'error'

export type ConnectedSession = {
	profile: ConnectionProfile
	connectedAt: string
	engineSessionId?: string | null
	capabilities?: EngineCapabilities | null
	/** Backend pool may drop while the workspace stays open. */
	liveStatus?: SessionLiveStatus
}
