import { EditorView } from '@codemirror/view'
import { forEachDiagnostic, setDiagnostics } from '@codemirror/lint'
import { describe, expect, it } from 'vitest'
import {
	editorEpoch,
	noteEditorEdit,
	placeDiagnostic,
	tokenEnd,
} from '../../src/lib/features/workspace/query/editor/query-diagnostic'
import { toIpcError } from '../../src/lib/core/ipc/error'
import { submissionStart } from '../../src/lib/features/workspace/query/editor/statement-span'

describe('query diagnostics', () => {
	it('underlines FORM inside statement 2', () => {
		const doc = 'SELECT 1; SELECT * FORM t'
		const form = doc.indexOf('FORM')
		const placed = placeDiagnostic(
			doc,
			0,
			{ offset: form, line: 1, column: form + 1 },
			'Expected end of statement',
		)
		expect(placed?.from).toBe(form)
		expect(doc.slice(placed!.from, placed!.to)).toBe('FORM')
	})

	it('shifts a statement offset by the editor anchor', () => {
		const doc = 'SELECT 1;\nSELECT * FORM t'
		const submitted = 'SELECT * FORM t'
		const anchor = doc.indexOf(submitted)
		const form = submitted.indexOf('FORM')
		const placed = placeDiagnostic(
			doc,
			anchor,
			{ offset: form, line: 1, column: form + 1 },
			'syntax',
		)
		expect(doc.slice(placed!.from, placed!.to)).toBe('FORM')
	})

	it('returns no range when the engine sent no position', () => {
		const fault = toIpcError({ kind: 'query', message: 'no such table' })
		expect(fault.position).toBeUndefined()
		expect(fault.message).toBe('no such table')
	})

	it('reads a positioned command error', () => {
		const fault = toIpcError({
			kind: 'query',
			message: 'syntax',
			position: { offset: 9, line: 1, column: 10 },
			statementIndex: 1,
		})
		expect(fault.statementIndex).toBe(1)
		expect(fault.position?.offset).toBe(9)
	})

	it('finds the submission start of statement 2', () => {
		const sql = 'SELECT 1; SELECT 2'
		const head = sql.indexOf('SELECT 2') + 1
		expect(submissionStart(sql, 'postgres', head, head, false)).toBe(
			sql.indexOf('SELECT 2'),
		)
		expect(tokenEnd('FORM t', 0)).toBe(4)
	})

	it('an edit after the run started is a new epoch', () => {
		const tab = 'query-diagnostic-epoch'
		const started = editorEpoch(tab)
		noteEditorEdit(tab)
		expect(editorEpoch(tab)).toBe(started + 1)
	})

	it('a diagnostic set without a lint source is still there after the idle delay', async () => {
		const host = document.createElement('div')
		document.body.append(host)
		const view = new EditorView({
			doc: 'SELECT * FORM t',
			parent: host,
		})
		const doc = view.state.doc.toString()
		const from = doc.indexOf('FORM')
		view.dispatch(
			setDiagnostics(view.state, [
				{
					from,
					to: from + 4,
					severity: 'error',
					message: 'syntax',
				},
			]),
		)
		await new Promise((resolve) => setTimeout(resolve, 800))
		const marked: string[] = []
		forEachDiagnostic(view.state, (_diagnostic, start, end) => {
			marked.push(view.state.doc.sliceString(start, end))
		})
		view.destroy()
		host.remove()
		expect(marked).toEqual(['FORM'])
	})
})