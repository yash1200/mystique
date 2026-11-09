/**
 * The editor's language mode, fetched per engine on first use: lang-sql and
 * the dialect for SQL engines, lang-javascript for MQL. A SQL connection
 * never downloads the JavaScript parser, and the reverse.
 */
import type { CatalogColumnRow } from '#core/contracts'
import { adapterFor } from '#engines'
import type { SQLNamespace } from '@codemirror/lang-sql'
import type { Extension } from '@codemirror/state'

type LanguageFor = (cols: CatalogColumnRow[]) => Extension

const ready = new Map<string, LanguageFor>()
const pending = new Map<string, Promise<LanguageFor>>()

const keyOf = (lang: string, engine: string) => `${lang}\0${engine}`

/** The language for this tab when its code has loaded, else undefined. */
export function peekLanguage(lang: string, engine: string): LanguageFor | undefined {
	return ready.get(keyOf(lang, engine))
}

/** Fetch the language code. Repeated calls share one request. */
export function loadLanguage(lang: string, engine: string): Promise<LanguageFor> {
	const key = keyOf(lang, engine)
	let job = pending.get(key)
	if (!job) {
		job = build(lang, engine).then(
			(make) => {
				ready.set(key, make)
				return make
			},
			(e: unknown) => {
				pending.delete(key)
				throw e
			},
		)
		pending.set(key, job)
	}
	return job
}

/** Language mode, dialect and completions all come from the engine adapter. */
async function build(lang: string, engine: string): Promise<LanguageFor> {
	const support = await adapterFor(engine).language.editor()
	if (lang === 'mql') {
		const { javascript, javascriptLanguage } = await import(
			'@codemirror/lang-javascript'
		)
		return (cols) => {
			const autocomplete = support.completion(cols)
			return [
				javascript(),
				...(autocomplete ? [javascriptLanguage.data.of({ autocomplete })] : []),
			]
		}
	}
	const { sql, StandardSQL } = await import('@codemirror/lang-sql')
	return (cols) => {
		const dialect = support.sqlDialect ?? StandardSQL
		const complete = support.sqlSchema?.(cols)
		const extra = support.completion(cols)
		const lang = sql({
			dialect,
			schema: (complete?.schema ?? {}) as SQLNamespace,
			defaultSchema: complete?.defaultSchema,
			upperCaseKeywords: true,
		})
		if (!extra) return lang
		return [lang, dialect.language.data.of({ autocomplete: extra })]
	}
}
