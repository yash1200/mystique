import { describe, expect, it } from 'vitest'
import {
	adminOpsApply,
	mergeCapabilities,
	objectKindRank,
	objectKindSpec,
	relationalCapabilities,
} from '../../src/lib/engines/capabilities'

const base = relationalCapabilities({ engine: 'x', sqlDialect: 'sql' })

describe('object kinds', () => {
	it('reads a declared kind from the plugin', () => {
		const caps = mergeCapabilities(
			{
				...base,
				objectKinds: [
					{ id: 'table', singular: 'table', plural: 'Tables', browsable: true, editable: true, primary: true },
					{ id: 'function', singular: 'function', plural: 'Functions', browsable: false, editable: false, primary: false },
				],
			},
			base,
		)
		const fn = objectKindSpec(caps, 'function')
		expect(fn.plural).toBe('Functions')
		expect(fn.browsable).toBe(false)
		expect(objectKindRank(caps, 'table')).toBeLessThan(objectKindRank(caps, 'function'))
	})

	it('keeps the adapter list when an older plugin sends none', () => {
		const caps = mergeCapabilities({ ...base, objectKinds: [] }, base)
		expect(caps.objectKinds.map((k) => k.id)).toEqual(['table', 'view'])
		expect(caps.paging).toBe('offset')
		expect(caps.supportsCatalogPaging).toBe(false)
	})

	it('treats an unknown kind as a grouped, definition-only object', () => {
		const spec = objectKindSpec(null, 'Trigger')
		expect(spec.id).toBe('trigger')
		expect(spec.primary).toBe(false)
		expect(spec.browsable).toBe(false)
		expect(spec.plural).toBe('Triggers')
		// Built-in kinds stay grids even without a plugin list.
		expect(objectKindSpec(null, 'view').browsable).toBe(true)
		expect(objectKindSpec(null, 'collection').editable).toBe(true)
	})

	it('offers rename, drop and truncate only where the backend allows them', () => {
		expect(adminOpsApply('table')).toBe(true)
		expect(adminOpsApply('view')).toBe(true)
		expect(adminOpsApply('collection')).toBe(true)
		expect(adminOpsApply('function')).toBe(false)
		expect(adminOpsApply('matview')).toBe(false)
	})

	it('accepts token paging and ignores nonsense', () => {
		expect(mergeCapabilities({ ...base, paging: 'token' }, base).paging).toBe('token')
		expect(mergeCapabilities({ ...base, paging: 'pages' }, base).paging).toBe('offset')
	})
})
