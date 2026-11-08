import assert from 'node:assert/strict'
import { test } from 'vitest'

import {
	encodeParam,
	paramHistory,
	rememberParamValue,
	scanQueryParams,
} from '../../src/lib/features/workspace/query/params'

test('a string literal does not open a parameter', () => {
	assert.deepEqual(scanQueryParams("SELECT ':id'", 'postgres').slots, [])
	assert.deepEqual(scanQueryParams('SELECT 1::int, : = 1', 'postgres').slots, [])
	assert.equal(scanQueryParams('SELECT :id', 'postgres').error, undefined)
	assert.deepEqual(
		scanQueryParams('SELECT :id', 'postgres').slots.map((slot) => slot.label),
		[':id'],
	)
})

test('names, questions, and dollar slots follow the server', () => {
	assert.deepEqual(
		scanQueryParams('SELECT :id, :id', 'postgres').slots.map((slot) => slot.label),
		[':id'],
	)
	assert.deepEqual(
		scanQueryParams('SELECT ?, ?', 'mysql').slots.map((slot) => slot.label),
		['?', '?'],
	)
	assert.deepEqual(
		scanQueryParams('SELECT $2', 'postgres').slots.map((slot) => slot.label),
		['$1', '$2'],
	)
	assert.equal(scanQueryParams('SELECT $0, $name', 'sqlite').slots.length, 0)
	assert.equal(scanQueryParams('SELECT ?|, ?&', 'postgres').slots.length, 0)
	assert.equal(
		scanQueryParams("SELECT $$ :id $$", 'postgres').slots.length,
		0,
	)
	assert.match(
		scanQueryParams('SELECT :id, $1', 'postgres').error ?? '',
		/Cannot mix :id with \$1/,
	)
	assert.equal(scanQueryParams('-- :id\nSELECT 1', 'postgres').slots.length, 0)
	assert.equal(scanQueryParams('# :id\nSELECT 1', 'mysql').slots.length, 0)
})

test('postgres jsonb ? operators and array slices are not parameters', () => {
	const labels = (sql: string, dialect: 'postgres' | 'mysql' | 'sqlite') =>
		scanQueryParams(sql, dialect).slots.map((slot) => slot.label)
	assert.deepEqual(
		labels("SELECT d ? 'a', d ?| array['a'], d ?& array['b'] FROM t", 'postgres'),
		[],
	)
	assert.deepEqual(
		labels(
			'SELECT arr[lo:hi], arr[:hi], arr[lo:], (f(x))[a:b], "c"[1:n], arr[lo::int:hi] FROM t',
			'postgres',
		),
		[],
	)
	assert.deepEqual(labels('SELECT ARRAY[:a, :b], x[(SELECT :i)], :after', 'postgres'), [
		':a',
		':b',
		':i',
		':after',
	])
	assert.deepEqual(labels('SELECT ?, :name', 'mysql'), ['?', ':name'])
	assert.deepEqual(labels('SELECT ?, :name', 'sqlite'), ['?', ':name'])
})

test('untouched text is missing and null is a value', () => {
	assert.equal(encodeParam('text', '', false), undefined)
	assert.equal(encodeParam('text', '', true), '')
	assert.equal(encodeParam('null', '', false), null)
	assert.equal(encodeParam('number', '4', true), 4)
	assert.equal(encodeParam('bool', 'false', true), false)
})

test('recent values stay on the parameter name', () => {
	localStorage.removeItem('mystique.paramHistory')
	rememberParamValue(':id', { kind: 'text', raw: 'secret-value-zz' })
	rememberParamValue(':id', { kind: 'number', raw: '7' })
	assert.equal(paramHistory(':id')[0]?.raw, '7')
	assert.equal(paramHistory(':id')[1]?.raw, 'secret-value-zz')
	assert.equal(paramHistory(':other').length, 0)
})
