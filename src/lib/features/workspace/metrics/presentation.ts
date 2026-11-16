/** View helpers for metric tiles and session rows. */
import type {
	ActiveBackendSession,
	MetricValue,
	SlowQuery,
} from '#core/contracts'

export function metricOccupancy(m: MetricValue): number | null {
	if (!m.available || m.value == null || m.max == null || m.max <= 0) {
		return null
	}
	return Math.max(0, Math.min(1, m.value / m.max))
}

/** Near its limit (≥ 85 % of the gauge) or a cache hit ratio under 80 %. */
export function metricWarns(m: MetricValue): boolean {
	if (!m.available) return false
	if (m.key === 'cacheHitRatio' && m.value != null && m.value < 80) return true
	const fill = metricOccupancy(m)
	return fill != null && fill >= 0.85
}

/** Running is info; waiting, locked or idle inside a transaction is a warning. */
export function sessionStateTone(
	state: string | null | undefined,
): 'warning' | 'info' | 'neutral' {
	const s = (state ?? '').toLowerCase()
	if (s.includes('idle in transaction') || s.includes('wait') || s.includes('lock')) {
		return 'warning'
	}
	if (s === 'active' || s === 'query') return 'info'
	return 'neutral'
}

export function backendSessionLabel(row: ActiveBackendSession): string {
	const who = row.user || 'unknown user'
	return `${who} (${row.id})`
}

export function filterSessions(
	rows: ActiveBackendSession[],
	query: string,
): ActiveBackendSession[] {
	const q = query.trim().toLowerCase()
	if (!q) return rows
	return rows.filter((s) =>
		[s.id, s.user, s.database, s.state, s.query]
			.filter(Boolean)
			.some((v) => String(v).toLowerCase().includes(q)),
	)
}

export function filterSlowQueries(
	rows: SlowQuery[],
	query: string,
): SlowQuery[] {
	const q = query.trim().toLowerCase()
	if (!q) return rows
	return rows.filter((s) =>
		[s.query, s.database, s.user, s.source]
			.filter(Boolean)
			.some((v) => String(v).toLowerCase().includes(q)),
	)
}
