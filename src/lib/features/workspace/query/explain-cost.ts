/**
 * Self cost is the parent's cost minus the sum of its children.
 * A missing number means the bar cannot be drawn.
 */
export function selfCostShare(
	cost: number | null | undefined,
	children: Array<number | null | undefined>,
): number | null {
	if (cost == null || !Number.isFinite(cost) || cost <= 0) return null
	let sum = 0
	for (const child of children) {
		if (child == null || !Number.isFinite(child)) return null
		sum += child
	}
	const self = Math.max(0, cost - sum)
	return Math.min(1, self / cost)
}
