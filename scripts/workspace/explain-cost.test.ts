import { describe, expect, it } from 'vitest'
import { selfCostShare } from '../../src/lib/features/workspace/query/explain/explain-cost'

describe('explain self cost', () => {
	it('a child that costs almost as much as the parent leaves a small self share', () => {
		expect(selfCostShare(100, [99])).toBeCloseTo(0.01)
	})

	it('a cheap child leaves most of the cost on the parent', () => {
		expect(selfCostShare(100, [10])).toBeCloseTo(0.9)
	})

	it('omits the bar when cost is missing', () => {
		expect(selfCostShare(null, [1])).toBeNull()
		expect(selfCostShare(10, [null])).toBeNull()
		expect(selfCostShare(0, [])).toBeNull()
	})
})
