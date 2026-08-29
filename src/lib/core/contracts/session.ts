import type { ConnectionProfile } from './connection'
import type { EngineCapabilities } from './engine-caps'

export type ConnectedSession = {
	profile: ConnectionProfile
	connectedAt: string
	engineSessionId?: string | null
	capabilities?: EngineCapabilities | null
}
