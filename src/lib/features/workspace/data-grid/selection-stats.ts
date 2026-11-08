/**
 * Status for the cells the grid already has. Callers pass those values only.
 */

export type SelectionStats = {
	count: number
	sum: number | null
	avg: number | null
}

function numericValue(value: unknown): number | null {
	if (typeof value === 'number') return Number.isFinite(value) ? value : null
	if (typeof value === 'boolean' || value == null) return null
	if (typeof value !== 'string') return null
	const text = value.trim()
	if (!/^[+-]?(?:\d+\.?\d*|\.\d+)$/.test(text)) return null
	const number = Number(text)
	if (!Number.isFinite(number)) return null
	if (!text.includes('.') && !Number.isSafeInteger(number)) return null
	return number
}

export function selectionStats(values: readonly unknown[]): SelectionStats {
	const numbers: number[] = []
	for (const value of values) {
		const number = numericValue(value)
		if (number != null) numbers.push(number)
	}
	if (numbers.length === 0) return { count: values.length, sum: null, avg: null }
	const sum = numbers.reduce((total, number) => total + number, 0)
	return { count: values.length, sum, avg: sum / numbers.length }
}

export function formatSelectionStats(stats: SelectionStats): string {
	if (stats.count === 0) return ''
	const count = `${stats.count} selected`
	if (stats.sum == null || stats.avg == null) return count
	const sum = formatStat(stats.sum)
	const avg = formatStat(stats.avg)
	return `${count} · SUM ${sum} · AVG ${avg}`
}

function formatStat(value: number): string {
	if (Number.isInteger(value)) return String(value)
	return String(Math.round(value * 1000) / 1000)
}
