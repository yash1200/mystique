import type {
	FetchServerMetricsRequest,
	ServerMetricsSnapshot,
	TerminateSessionRequest,
	TerminateSessionResult,
} from '$core/contracts'
import { fetchEngineServerMetrics, terminateEngineSession } from '$core/ipc'
import { runWithWriteGuard } from '$features/workspace/shell'

export function fetchServerMetrics(
	request: FetchServerMetricsRequest,
): Promise<ServerMetricsSnapshot> {
	return fetchEngineServerMetrics(request)
}

export function terminateBackendSession(
	request: TerminateSessionRequest,
): Promise<TerminateSessionResult> {
	return runWithWriteGuard(request.sessionId, () =>
		terminateEngineSession(request),
	)
}
