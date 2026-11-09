import { describe, expect, test } from 'vitest'

import type { CatalogColumnRow } from '../../src/lib/core/contracts'
import {
	resultColumnSources,
	resultColumnTitle,
} from '../../src/lib/engines/sql/result-source'

function col(schema: string, table: string, column: string, dataType = 'text'): CatalogColumnRow {
	return { schema, table, column, dataType, nullable: true, pk: false }
}

const rows = [
	col('public', 'users', 'id', 'int4'),
	col('public', 'users', 'email'),
	col('public', 'orders', 'id', 'int4'),
	col('public', 'orders', 'user_id', 'int4'),
	col('public', 'orders', 'total', 'numeric'),
	col('audit', 'users', 'id', 'int8'),
]

const label = (s: ReturnType<typeof resultColumnSources>[number]) =>
	s ? `${s.schema}.${s.table}.${s.column}` : null

describe('result column sources', () => {
	test('star and plain columns of one table', () => {
		expect(resultColumnSources('SELECT * FROM users', ['id', 'email'], rows).map(label)).toEqual([
			'public.users.id',
			'public.users.email',
		])
		expect(
			resultColumnSources('select email AS mail, id from public.users where id > 1', ['mail', 'id'], rows).map(label),
		).toEqual(['public.users.email', 'public.users.id'])
	})

	test('joins: aliases, qualified stars, and ambiguous names', () => {
		const sql = `SELECT u.email, o.*, id, count(*) AS n
			FROM users u JOIN orders AS o ON o.user_id = u.id
			GROUP BY 1`
		const names = ['email', 'id', 'user_id', 'total', 'id', 'n']
		expect(resultColumnSources(sql, names, rows).map(label)).toEqual([
			'public.users.email',
			'public.orders.id',
			'public.orders.user_id',
			'public.orders.total',
			null, // `id` is in both tables
			null, // an expression
		])
	})

	test('schema-qualified and quoted names', () => {
		expect(
			resultColumnSources('SELECT "id" FROM "audit"."users"', ['id'], rows)[0],
		).toMatchObject({ schema: 'audit', table: 'users', dataType: 'int8' })
	})

	test('unknown shapes stay unknown', () => {
		expect(resultColumnSources('SELECT * FROM (SELECT 1) s', ['?column?'], rows)).toEqual([null])
		expect(resultColumnSources('SELECT 1', ['?column?'], rows)).toEqual([null])
		expect(resultColumnSources('SELECT * FROM nope', ['a'], rows)).toEqual([null])
		// A name the engine renamed is not trusted.
		expect(resultColumnSources('SELECT id FROM users', ['other'], rows)).toEqual([null])
	})

	test('a CTE body does not count as the top-level select', () => {
		const sql = 'WITH x AS (SELECT id FROM orders) SELECT email FROM users'
		expect(resultColumnSources(sql, ['email'], rows).map(label)).toEqual(['public.users.email'])
	})

	test('tooltip text', () => {
		const src = { schema: 'public', table: 'users', column: 'email', dataType: 'text' }
		expect(resultColumnTitle('mail', 'varchar', src)).toBe('public.users.email\nas mail\nType: varchar')
		expect(resultColumnTitle('n', null, null)).toBe('n')
		expect(resultColumnTitle('n', 'int8', null)).toBe('n\nType: int8')
	})
})
