import { describe, expect, it } from 'vitest'
import type { TableDefinition } from '../../src/lib/core/contracts/ddl'
import { menuDangerPrompt } from '../../src/lib/features/workspace/explorer/danger'
import { draftToSend } from '../../src/lib/features/workspace/explorer/structure-edit'
import { importDialogCopy } from '../../src/lib/features/workspace/transfer/import-dialog'
import { guardCopy } from '../../src/lib/features/workspace/session/guard-copy'

describe('drop / truncate from the explorer', () => {
	const base = { name: 'orders', noun: 'table', connectionName: 'Shop' }

	it('local and staging: one dialog that pre-confirms the name', () => {
		for (const environment of ['local', 'staging']) {
			const p = menuDangerPrompt({ ...base, action: 'drop', environment })
			expect(p?.kind).toBe('confirm')
			// The guard loop sends this back, so the backend does not ask again.
			expect(p?.objectName).toBe('orders')
			expect(p && guardCopy(p).confirm).toBe('Drop')
			expect(p && guardCopy(p).title).toBe('Drop table')
		}
		const truncate = menuDangerPrompt({ ...base, action: 'truncate', environment: 'local' })
		expect(truncate && guardCopy(truncate).body).toBe('Delete every row in “orders”.')
	})

	it('production: no menu dialog; the backend type-to-confirm is the one dialog', () => {
		expect(menuDangerPrompt({ ...base, action: 'drop', environment: 'production' })).toBeNull()
		expect(
			menuDangerPrompt({ ...base, action: 'truncate', environment: ' Production ' }),
		).toBeNull()
	})
})

describe('import dialog', () => {
	it('labels the run on production and adds nothing else', () => {
		const prod = importDialogCopy({
			objectName: 'orders',
			production: true,
			busy: false,
			hasPreview: true,
		})
		expect(prod.confirm).toBe('Import into production')
		expect(prod.title).toBe('Import into production · orders')
		const local = importDialogCopy({
			objectName: 'orders',
			production: false,
			busy: false,
			hasPreview: true,
		})
		expect(local.confirm).toBe('Import')
		expect(
			importDialogCopy({ objectName: 'o', production: false, busy: true, hasPreview: false })
				.confirm,
		).toBe('Reading…')
	})
})

describe('structure auto-plan', () => {
	it('sends a plain copy with the validator text for collections', () => {
		const draft: TableDefinition = { name: 'users', kind: 'collection', columns: [] }
		const sent = draftToSend(draft, ' {"$jsonSchema": {}} ')
		expect(sent.validator).toBe(' {"$jsonSchema": {}} ')
		expect(draft.validator).toBeUndefined()
		expect(draftToSend(draft, '   ').validator).toBeNull()
		const table: TableDefinition = { name: 't', kind: 'table', columns: [] }
		expect(draftToSend(table, 'ignored').validator).toBeUndefined()
	})
})
