import assert from 'node:assert/strict'
import { test } from 'vitest'

import {
	buildInList,
	buildInsertSql,
	buildUpdateSql,
	buildWhereSql,
	identStyleFor,
	quoteIdent,
	sqlLiteral,
} from '../../src/lib/features/workspace/data-grid/copy-as.ts'
import {
	layoutColumns,
	moveColumn,
	toggleFrozen,
	type GridColumnLayout,
} from '../../src/lib/features/workspace/data-grid/column-layout.ts'
import { columnCompletions } from '../../src/lib/features/workspace/table/filters/column-complete.ts'
import { pushUndo, undoEdit, type GridEditSnapshot } from '../../src/lib/features/workspace/table/edit-history.ts'
import { gridShortcutBlocked } from '../../src/lib/features/workspace/data-grid/keyboard.ts'
import { applyCellBatch } from '../../src/lib/features/workspace/table/paste-batch.ts'
import {
	formatSelectionStats,
	selectionStats,
} from '../../src/lib/features/workspace/data-grid/selection-stats.ts'

const docs = [
	{ name: 'id', dataType: 'int8', pk: true },
	{ name: 'body', dataType: 'text', pk: false },
]

test('copy as INSERT quotes identifiers and keeps a quote inside the literal', () => {
	assert.equal(identStyleFor('sqlite', 'sqlite'), 'postgres')
	assert.equal(identStyleFor('mysql', 'mysql'), 'mysql')
	assert.equal(identStyleFor('mariadb', ''), 'mysql')
	const pg = buildInsertSql('postgres', 'docs', docs, [[1, "a'); DROP TABLE docs;--"]])
	assert.match(pg, /^INSERT INTO "docs" \("id", "body"\) VALUES \(1, 'a''\); DROP TABLE docs;--'\);$/)
	assert.equal(pg.includes("VALUES (1, 'a'); DROP"), false)
	const mysql = buildInsertSql('mysql', 'docs', docs, [['9007199254740993', 'x']])
	assert.equal(
		mysql,
		'INSERT INTO `docs` (`id`, `body`) VALUES (9007199254740993, \'x\');',
	)
	assert.equal(quoteIdent('postgres', 'we"ird'), '"we""ird"')
	assert.equal(sqlLiteral('mysql', "a\\b'c", 'text'), "'a\\\\b''c'")
})

test('copy as UPDATE, IN, and WHERE use the primary key', () => {
	const update = buildUpdateSql('postgres', 'docs', docs, [[2, 'b']])
	assert.equal(
		update,
		'UPDATE "docs" SET "body" = \'b\' WHERE "id" = 2;',
	)
	const list = buildInList('postgres', [docs[1]!], [['a'], ['b']])
	assert.equal(list, `"body" IN ('a', 'b')`)
	const where = buildWhereSql('postgres', docs, [[1, 'a']])
	assert.equal(where, `WHERE ("id" = 1 AND "body" = 'a')`)
})

test('three numeric cells show count and SUM', () => {
	const stats = selectionStats([1, '2', 3, 'note'])
	assert.equal(stats.count, 4)
	assert.equal(stats.sum, 6)
	assert.equal(stats.avg, 2)
	assert.match(formatSelectionStats(selectionStats([1, 2, 3])), /3 selected/)
	assert.match(formatSelectionStats(selectionStats([1, 2, 3])), /SUM 6/)
	assert.equal(selectionStats(['9007199254740993']).sum, null)
	assert.equal(selectionStats(['note']).sum, null)
})

test('undo restores a pending edit and stops at the cap', () => {
	const base: GridEditSnapshot = {
		rows: [{ id: 1, n: 10 }],
		originals: [{ id: 1, n: 10 }],
		dirtyKeys: [],
		newRows: [],
		deleted: [],
	}
	const edited: GridEditSnapshot = {
		...base,
		rows: [{ id: 1, n: 11 }],
		dirtyKeys: ['0::n'],
	}
	const undone = undoEdit(pushUndo([], base), [], edited)
	assert.ok(undone)
	assert.equal(undone.current.rows[0]?.n, 10)
	assert.equal(undone.redo.length, 1)
	let stack: GridEditSnapshot[] = []
	for (let i = 0; i < 120; i++) {
		stack = pushUndo(stack, { ...base, dirtyKeys: [String(i)] })
	}
	assert.equal(stack.length, 100)
	assert.equal(stack[0]?.dirtyKeys[0], '20')
})

test('a filter input and an open modal block grid shortcuts', () => {
	assert.equal(
		gridShortcutBlocked({ modalOpen: true, inCellEditor: false, tagName: 'DIV' }),
		true,
	)
	assert.equal(
		gridShortcutBlocked({
			modalOpen: false,
			inCellEditor: false,
			tagName: 'INPUT',
		}),
		true,
	)
	assert.equal(
		gridShortcutBlocked({
			modalOpen: false,
			inCellEditor: true,
			tagName: 'INPUT',
		}),
		false,
	)
})

test('paste of 1k cells assigns the row array once', () => {
	const rows = Array.from({ length: 50 }, (_, row) => {
		const record: Record<string, unknown> = {}
		for (let col = 0; col < 20; col++) record[`c${col}`] = 0
		return record
	})
	const edits = []
	for (let row = 0; row < 50; row++) {
		for (let col = 0; col < 20; col++) {
			edits.push({ row, col: `c${col}`, value: row * 20 + col })
		}
	}
	assert.equal(edits.length, 1000)
	let calls = 0
	const dirty = applyCellBatch(rows, rows, new Set(), edits, (next) => {
		calls += 1
		assert.equal(next[49]?.c19, 999)
		assert.notEqual(next, rows)
	})
	assert.equal(calls, 1)
	assert.equal(dirty.size, 999)
	assert.equal(dirty.has('0::c0'), false)
})

test('freeze, hide, and reorder are per column name', () => {
	const columns = [{ name: 'id' }, { name: 'body' }, { name: 'n' }]
	let layout: GridColumnLayout = { order: [], hidden: [], frozen: [] }
	layout = toggleFrozen(layout, 'id')
	layout = moveColumn(layout, ['id', 'body', 'n'], 'n', -1)
	const placed = layoutColumns(columns, { ...layout, hidden: ['body'] })
	assert.deepEqual(
		placed.frozen.map((column) => column.name),
		['id'],
	)
	assert.deepEqual(
		placed.scroll.map((column) => column.name),
		['n'],
	)
})

test('the WHERE box completes column names from the typed prefix', () => {
	const code = 'status = 1 AND bo'
	const found = columnCompletions(code, code.length, ['id', 'body', 'n'])
	assert.deepEqual(found.items, ['body'])
	assert.equal(found.start, code.length - 2)
})

test('an INSERT from the grid round-trips through SQLite', async () => {
	const sql = buildInsertSql(
		'postgres',
		'docs',
		docs,
		[[1, "a'); DROP TABLE docs;--"]],
	)
	const sqlite = await import('node:sqlite').catch(() => null)
	if (!sqlite) return
	const db = new sqlite.DatabaseSync(':memory:')
	db.exec('CREATE TABLE docs (id INTEGER PRIMARY KEY, body TEXT)')
	db.exec(sql)
	const row = db.prepare('SELECT body FROM docs WHERE id = 1').get() as { body: string }
	assert.equal(row.body, "a'); DROP TABLE docs;--")
	const count = db.prepare("SELECT COUNT(*) AS n FROM sqlite_master WHERE name = 'docs'").get() as {
		n: number
	}
	assert.equal(count.n, 1)
	db.close()
})
