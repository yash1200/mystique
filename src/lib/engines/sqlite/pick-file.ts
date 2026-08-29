/**
 * Native file picker for user SQLite database paths.
 * Tauri desktop only — browser previews cannot expose full filesystem paths.
 */
import { isTauri } from '$core/ipc'
export type PickSqliteResult =
	| { ok: true; path: string }
	| { ok: false; cancelled: true }
	| { ok: false; cancelled: false; error: string }

const SQLITE_FILTERS = [
	{ name: 'SQLite', extensions: ['sqlite', 'db', 'sqlite3', 'db3'] },
	{ name: 'All files', extensions: ['*'] },
]

/** Common extensions accepted for typed paths (case-insensitive). */
export const SQLITE_PATH_EXTENSIONS = [
	'.sqlite',
	'.sqlite3',
	'.db',
	'.db3',
] as const

/**
 * Soft check for a path that looks like a SQLite file.
 * Empty → false. No extension → allowed (SQLite often uses bare names).
 * Known extensions → true. Other extensions → false.
 */
export function isLikelySqlitePath(path: string): boolean {
	const t = path.trim()
	if (!t) return false
	const base = (t.split(/[/\\]/).pop() ?? t).toLowerCase()
	const dot = base.lastIndexOf('.')
	if (dot <= 0) return true // no extension, or leading-dot name
	const ext = base.slice(dot)
	return (SQLITE_PATH_EXTENSIONS as readonly string[]).includes(ext)
}

/**
 * Open a native open-dialog for a single SQLite database file.
 * Returns the absolute path, or cancelled / error.
 */
export async function pickSqliteDatabasePath(
	defaultPath?: string,
): Promise<PickSqliteResult> {
	if (!isTauri()) {
		return {
			ok: false,
			cancelled: false,
			error:
				'File picker is available in the desktop app. Paste a full path while using browser preview.',
		}
	}

	try {
		const { open } = await import('@tauri-apps/plugin-dialog')
		const selected = await open({
			title: 'Select SQLite database',
			multiple: false,
			directory: false,
			filters: SQLITE_FILTERS,
			defaultPath: defaultPath?.trim() || undefined,
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
			error: e instanceof Error ? e.message : String(e),
		}
	}
}
