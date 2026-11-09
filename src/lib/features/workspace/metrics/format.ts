/** Display helpers for the engine-agnostic metrics contract. */
import type { MetricValue } from '#core/contracts'

function formatBytes(n: number): string {
	const abs = Math.abs(n)
	if (abs < 1024) return `${Math.round(n)} B`
	if (abs < 1024 ** 2) return `${trimNum(n / 1024)} KB`
	if (abs < 1024 ** 3) return `${trimNum(n / 1024 ** 2)} MB`
	if (abs < 1024 ** 4) return `${trimNum(n / 1024 ** 3)} GB`
	return `${trimNum(n / 1024 ** 4)} TB`
}

export function formatDurationMs(ms: number): string {
	const n = Math.max(0, Math.round(ms))
	if (n < 1000) return `${n} ms`
	const sec = n / 1000
	if (sec < 60) return `${trimNum(sec)} s`
	const min = Math.floor(sec / 60)
	const remSec = Math.round(sec % 60)
	if (min < 60) return remSec ? `${min}m ${remSec}s` : `${min}m`
	const hr = Math.floor(min / 60)
	const remMin = min % 60
	if (hr < 48) return remMin ? `${hr}h ${remMin}m` : `${hr}h`
	const days = Math.floor(hr / 24)
	const remHr = hr % 24
	return remHr ? `${days}d ${remHr}h` : `${days}d`
}

function formatCount(n: number): string {
	if (!Number.isFinite(n)) return '—'
	if (Math.abs(n) >= 1_000_000) return `${trimNum(n / 1_000_000)}M`
	if (Math.abs(n) >= 10_000) return `${trimNum(n / 1_000)}k`
	if (Number.isInteger(n)) return n.toLocaleString()
	return trimNum(n)
}

function formatPercent(n: number): string {
	if (!Number.isFinite(n)) return '—'
	const digits = Math.abs(n) >= 10 ? 1 : 2
	return `${n.toFixed(digits)}%`
}

export function formatMetric(m: MetricValue): string {
	if (!m.available) return '—'
	if (m.display?.trim()) return m.display.trim()
	const v = m.value
	if (v == null || !Number.isFinite(v)) return '—'
	if (m.kind === 'bytes') return formatBytes(v)
	if (m.kind === 'durationMs') return formatDurationMs(v)
	if (m.kind === 'percent' || m.kind === 'ratio') return formatPercent(v)
	return formatCount(v)
}

export function formatMetricHint(m: MetricValue): string | null {
	if (!m.available) return m.unavailableReason?.trim() || 'Not available'
	if (m.max != null && m.value != null && Number.isFinite(m.max)) {
		return `of ${formatCount(m.max)}`
	}
	return null
}

function trimNum(n: number): string {
	const abs = Math.abs(n)
	const digits = abs >= 100 ? 0 : abs >= 10 ? 1 : 2
	return n
		.toFixed(digits)
		.replace(/\.0+$/, '')
		.replace(/(\.\d*[1-9])0+$/, '$1')
}
