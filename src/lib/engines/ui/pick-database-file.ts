/**
 * Native file picker for file-kind connections (SQLite, later DuckDB).
 */
import { errorMessage } from '#core/ipc'

export type PickDatabaseFileResult =
	| { ok: true; path: string }
	| { ok: false; cancelled: true }
	| { ok: false; cancelled: false; error: string }

export type FilePickerFilter = { name: string; extensions: string[] }

const SQLITE_FILTERS: FilePickerFilter[] = [
	{ name: 'SQLite', extensions: ['sqlite', 'db', 'sqlite3', 'db3'] },
	{ name: 'All files', extensions: ['*'] },
]

export async function pickDatabaseFilePath(opts?: {
	defaultPath?: string
	title?: string
	filters?: FilePickerFilter[]
}): Promise<PickDatabaseFileResult> {
	try {
		const { open } = await import('@tauri-apps/plugin-dialog')
		const selected = await open({
			title: opts?.title ?? 'Select database file',
			multiple: false,
			directory: false,
			filters: opts?.filters ?? SQLITE_FILTERS,
			defaultPath: opts?.defaultPath?.trim() || undefined,
		})

		if (selected == null) return { ok: false, cancelled: true }
		if (Array.isArray(selected)) {
			const first = selected[0]
			if (!first) return { ok: false, cancelled: true }
			return { ok: true, path: first }
		}
		return { ok: true, path: selected }
	} catch (e) {
		return {
			ok: false,
			cancelled: false,
			error: errorMessage(e),
		}
	}
}
