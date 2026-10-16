import { describe, expect, it } from 'vitest'
import {
	aggregateStageLabels,
	insideAggregate,
	stagePosition,
} from '../../src/lib/engines/mongodb/complete'

describe('mongo aggregate completion', () => {
	it('offers stages inside an aggregate call', () => {
		const prefix = 'db.orders.aggregate([{ $'
		expect(insideAggregate(prefix)).toBe(true)
		expect(stagePosition(prefix)).toBe(true)
		expect(aggregateStageLabels(prefix)).toEqual(
			expect.arrayContaining(['$match', '$group', '$limit']),
		)
	})

	it('offers stages after a stage comma', () => {
		const prefix = 'db.orders.aggregate([{ $match: {} }, { '
		expect(insideAggregate(prefix)).toBe(true)
		expect(stagePosition(prefix)).toBe(true)
	})

	it('offers stages on the next line of a pipeline', () => {
		const prefix = 'db.orders.aggregate([\n  { $'
		expect(insideAggregate(prefix)).toBe(true)
		expect(stagePosition(prefix)).toBe(true)
		expect(aggregateStageLabels(prefix)).toContain('$match')
	})

	it('does not treat a field inside $match as a stage', () => {
		const prefix = 'db.orders.aggregate([{ $match: { '
		expect(insideAggregate(prefix)).toBe(true)
		expect(stagePosition(prefix)).toBe(false)
		expect(aggregateStageLabels(prefix)).toBeNull()
	})

	it('does not treat a find filter as a pipeline', () => {
		const prefix = 'db.orders.find({ $'
		expect(insideAggregate(prefix)).toBe(false)
		expect(aggregateStageLabels(prefix)).toBeNull()
	})
})
