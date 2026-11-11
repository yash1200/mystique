import assert from 'node:assert/strict'
import { test } from 'vitest'

import { tableNameError } from '../../src/lib/features/workspace/explorer/table-name'

const base = { current: 'prompts', noun: 'table', schema: 'ai_platform', taken: ['prompts', 'llms'] }

test('a rename needs a new, free name', () => {
	assert.equal(tableNameError({ ...base, action: 'rename', next: '  ' }), 'Give the table a name.')
	assert.equal(tableNameError({ ...base, action: 'rename', next: 'prompts' }), null, 'unchanged: no error, just disabled')
	assert.equal(tableNameError({ ...base, action: 'rename', next: 'LLMS' }), 'ai_platform.LLMS already exists.')
	assert.equal(tableNameError({ ...base, action: 'rename', next: 'prompt_templates' }), null)
})

test('a duplicate cannot keep the original name', () => {
	assert.equal(
		tableNameError({ ...base, action: 'duplicate', next: 'prompts' }),
		'Pick a name other than prompts.',
	)
	assert.equal(tableNameError({ ...base, action: 'duplicate', next: 'prompts_copy' }), null)
})
