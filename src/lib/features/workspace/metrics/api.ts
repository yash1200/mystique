import type {
	FetchServerMetricsRequest,
	ServerMetricsSnapshot,
	TerminateSessionRequest,
	TerminateSessionResult,
} from '#core/contracts'
import { fetchEngineServerMetrics, terminateEngineSession } from '#core/ipc'

export function fetchServerMetrics(
	request: FetchServerMetricsRequest,
): Promise<ServerMetricsSnapshot> {
	return fetchEngineServerMetrics(request)
}

/** The End session dialog is the only confirmation; the backend never unlocks for this. */
export function terminateBackendSession(
	request: TerminateSessionRequest,
): Promise<TerminateSessionResult> {
	return terminateEngineSession(request)
}
