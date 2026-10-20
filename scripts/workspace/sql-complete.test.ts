import { CompletionContext } from '@codemirror/autocomplete'
import { MySQL, PostgreSQL, sql } from '@codemirror/lang-sql'
import { ensureSyntaxTree } from '@codemirror/language'
import { EditorState } from '@codemirror/state'
import assert from 'node:assert/strict'
import { test } from 'vitest'

import type { CatalogColumnRow } from '../../src/lib/core/contracts/engines'
import {
	catalogSignature,
	sqlCatalogCompletion,
	sqlSchemaFromColumns,
} from '../../src/lib/engines/sql/complete'
import {
	expandSelectStar,
	scopeCompletions,
} from '../../src/lib/engines/sql/scope'

function column(
	schema: string,
	table: string,
	name: string,
): CatalogColumnRow {
	return {
		schema,
		table,
		column: name,
		dataType: 'text',
		nullable: true,
	}
}

function applyOf(node: unknown): string | undefined {
	if (!node || typeof node !== 'object' || !('self' in node)) return undefined
	const self = (node as { self?: { apply?: unknown } }).self
	return typeof self?.apply === 'string' ? self.apply : undefined
}

test('catalog idents are inserted quoted on postgres and mysql', () => {
	const rows = [
		column('public', 'Orders', 'id'),
		column('public', 'user', 'id'),
		column('public', 'createdAt', 'id'),
	]
	const pg = sqlSchemaFromColumns(rows, {
		nestSchemas: true,
		quote: 'postgres',
	})
	const nested = pg.schema.public as {
		children: Record<string, unknown>
	}
	assert.equal(applyOf(nested.children.Orders), '"Orders"')
	assert.equal(applyOf(nested.children.user), '"user"')
	assert.equal(applyOf(nested.children.createdAt), '"createdAt"')

	const my = sqlSchemaFromColumns(
		rows.map((row) => ({ ...row, schema: 'app' })),
		{ nestSchemas: false, quote: 'mysql' },
	)
	assert.equal(applyOf(my.schema.Orders), '`Orders`')
	assert.equal(applyOf(my.schema.user), '`user`')
	assert.equal(applyOf(my.schema.createdAt), '`createdAt`')
})

test('mysql does not merge columns of the same table in two schemas', () => {
	const cfg = sqlSchemaFromColumns(
		[column('shop', 'users', 'id'), column('blog', 'users', 'email')],
		{ nestSchemas: false, quote: 'mysql' },
	)
	const shop = cfg.schema['shop.users'] as { children: { label: string }[] }
	const blog = cfg.schema['blog.users'] as { children: { label: string }[] }
	assert.deepEqual(
		shop.children.map((child) => child.label),
		['id'],
	)
	assert.deepEqual(
		blog.children.map((child) => child.label),
		['email'],
	)
})

test('a cte column and a table alias complete', () => {
	const rows = [
		column('public', 't', 'a'),
		column('public', 't', 'b'),
		column('public', 'users', 'id'),
		column('public', 'users', 'email'),
	]
	const cte = 'WITH c AS (SELECT a, b FROM t) SELECT '
	const cteHits = scopeCompletions(cte, cte.length, rows, 'postgres')
	assert.ok(cteHits)
	assert.deepEqual(
		cteHits.options.map((option) => option.label),
		['c.a', 'c.b'],
	)
	assert.equal(cteHits.options[0]?.apply, '"c"."a"')

	const alias = 'SELECT u. FROM users u'
	const at = alias.indexOf('u.') + 2
	const aliasHits = scopeCompletions(alias, at, rows, 'mysql')
	assert.ok(aliasHits)
	assert.deepEqual(
		aliasHits.options.map((option) => option.label),
		['id', 'email'],
	)
	assert.equal(aliasHits.options[0]?.apply, '`id`')
})

test('expand star replaces the star with the quoted column list', () => {
	const rows = [column('public', 't', 'id'), column('public', 't', 'name')]
	assert.equal(
		expandSelectStar('SELECT * FROM t', rows, 'postgres'),
		'SELECT "id", "name" FROM t',
	)
	assert.equal(
		expandSelectStar('SELECT * FROM t', rows, 'mysql'),
		'SELECT `id`, `name` FROM t',
	)
	assert.equal(expandSelectStar('SELECT COUNT(*) FROM t', rows, 'postgres'), null)
})

test('snippets come from the dialect and stay out of strings', async () => {
	const doc = 'SELECT '
	const state = EditorState.create({
		doc,
		extensions: [sql({ dialect: PostgreSQL })],
	})
	ensureSyntaxTree(state, doc.length)
	const src = sqlCatalogCompletion([], {
		dialect: PostgreSQL,
		kind: 'postgresql',
		nestSchemas: true,
	})
	const offered = await src(new CompletionContext(state, doc.length, true))
	assert.ok(offered && !Array.isArray(offered))
	const select = offered.options.find((option) => option.label === 'SELECT * FROM')
	assert.equal(typeof select?.apply, 'function')

	const literal = "SELECT ':id'"
	const inside = EditorState.create({
		doc: literal,
		extensions: [sql({ dialect: PostgreSQL })],
	})
	ensureSyntaxTree(inside, literal.length)
	const hidden = await src(
		new CompletionContext(inside, literal.indexOf(':id') + 1, true),
	)
	assert.equal(hidden, null)

	const mysqlDoc = 'SELECT '
	const mysqlState = EditorState.create({
		doc: mysqlDoc,
		extensions: [sql({ dialect: MySQL })],
	})
	ensureSyntaxTree(mysqlState, mysqlDoc.length)
	const mysqlSrc = sqlCatalogCompletion([], {
		dialect: MySQL,
		kind: 'mysql',
		nestSchemas: false,
	})
	const mysqlOffer = await mysqlSrc(
		new CompletionContext(mysqlState, mysqlDoc.length, true),
	)
	assert.ok(mysqlOffer && !Array.isArray(mysqlOffer))
	assert.ok(mysqlOffer.options.some((option) => option.label === 'SHOW TABLES'))
	assert.equal(
		mysqlOffer.options.some((option) => option.label === 'ILIKE'),
		false,
	)
})

test('completing user on postgres inserts a quoted ident', async () => {
	const rows = [column('public', 'user', 'id')]
	const doc = 'SELECT * FROM '
	const state = EditorState.create({
		doc,
		extensions: [sql({ dialect: PostgreSQL })],
	})
	ensureSyntaxTree(state, doc.length)
	const src = sqlCatalogCompletion(rows, {
		dialect: PostgreSQL,
		kind: 'postgresql',
		nestSchemas: true,
	})
	const result = await src(new CompletionContext(state, doc.length, true))
	assert.ok(result && !Array.isArray(result))
	const user = result.options.find((option) => option.label === 'user')
	assert.equal(user?.apply, '"user"')
})

test('catalog signature ignores a new array with the same columns', () => {
	const left = [column('public', 't', 'id')]
	const right = [column('public', 't', 'id')]
	assert.equal(catalogSignature(left), catalogSignature(right))
	assert.notEqual(
		catalogSignature(left),
		catalogSignature([column('public', 't', 'name')]),
	)
})
