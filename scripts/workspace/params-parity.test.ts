/**
 * The editor's parameter scanner against the corpus `mystique-sql` tests
 * (crates/mystique-sql/tests/params_fixtures.rs). Same files, same answers.
 */
import { readdirSync, readFileSync } from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import type { SqlSplitDialect } from '../../src/lib/engines/sql/split'
import { scanQueryParams } from '../../src/lib/features/workspace/query/params/params'

const dir = path.resolve(__dirname, '../../crates/mystique-sql/tests/fixtures/params')
const expected = JSON.parse(readFileSync(path.join(dir, 'expected.json'), 'utf8')) as Record<
	string,
	string[]
>
const files = readdirSync(dir)
	.filter((name) => name.endsWith('.sql'))
	.sort()

function dialectOf(file: string): SqlSplitDialect {
	const dialect = file.slice(0, -'.sql'.length).split('.').pop()
	if (
		dialect === 'postgres' ||
		dialect === 'mysql' ||
		dialect === 'sqlite' ||
		dialect === 'generic'
	) {
		return dialect
	}
	throw new Error(`${file}: unknown dialect ${dialect}`)
}

describe('parameter scanner parity corpus', () => {
	it('lists every fixture and nothing else', () => {
		expect(files.length).toBeGreaterThan(0)
		expect(Object.keys(expected).sort()).toEqual(files)
	})

	it.each(files)('%s', (file) => {
		const sql = readFileSync(path.join(dir, file), 'utf8')
		const scan = scanQueryParams(sql, dialectOf(file))
		expect(scan.error).toBeUndefined()
		expect(scan.slots.map((slot) => slot.label)).toEqual(expected[file])
	})
})
