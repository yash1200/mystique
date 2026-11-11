import { describe, expect, it } from 'vitest'
import { highlightCode } from '../../src/lib/core/util/highlight'

const kinds = (src: string) =>
	highlightCode('dbml', src)
		.filter((t) => t.text.trim())
		.map((t) => `${t.kind}:${t.text.trim()}`)

describe('DBML highlighting', () => {
	it('separates tables, columns, types and settings', () => {
		const out = kinds('Table "ai.prompts" {\n  id int4 [primary key, not null]\n  name varchar(64)\n}')
		expect(out).toContain('keyword:Table')
		expect(out).toContain('func:"ai.prompts"')
		expect(out).toContain('text:id')
		expect(out).toContain('type:int4')
		expect(out).toContain('atom:primary')
		expect(out).toContain('atom:null')
		expect(out).toContain('type:varchar')
		expect(out).toContain('number:64')
	})

	it('colours refs: tables as names, columns plain, the arrow as an operator', () => {
		const out = kinds('Ref: prompts.user_id > users.id // fk')
		expect(out).toEqual([
			'keyword:Ref',
			'punct::',
			'func:prompts',
			'punct:.',
			'text:user_id',
			'operator:>',
			'func:users',
			'punct:.',
			'text:id',
			'comment:// fk',
		])
	})
})
