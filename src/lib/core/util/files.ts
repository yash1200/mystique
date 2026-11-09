/**
 * Save / pick files from the desktop shell.
 * Native dialogs and file writes through the Tauri plugins.
 * Domain-free: knows about bytes and paths, never about rows or engines.
 */
import type { TableFileFormat } from '#core/contracts'
export type SaveResult =
	| { ok: true; path: string }
	| { ok: false; cancelled: true }
	| { ok: false; cancelled: false; error: string }

export type { TableFileFormat }

export function safeExportFileBase(name: string): string {
	const base = (name || 'export').trim() || 'export'
	return (
		base
			.replace(/[^\w.\-]+/g, '_')
			.replace(/_+/g, '_')
			.replace(/^_|_$/g, '')
			.slice(0, 80) || 'export'
	)
}

export function safeFileBase(name: string): string {
	return safeExportFileBase(name)
}

export function extensionForFormat(format: TableFileFormat): string {
	if (format === 'jsonl') return 'jsonl'
	if (format === 'sql') return 'sql'
	if (format === 'json') return 'json'
	return 'csv'
}

/** Strip a known export suffix and attach the suffix for `format`. */
export function applyFormatExtension(
	name: string,
	format: TableFileFormat,
): string {
	const stripped = name.trim().replace(/\.(csv|jsonl|json|sql)$/i, '')
	return `${safeExportFileBase(stripped || 'export')}.${extensionForFormat(format)}`
}

export function shortStamp(): string {
	const d = new Date()
	const p = (n: number) => String(n).padStart(2, '0')
	return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}`
}

export async function saveBytesWithPrompt(
	bytes: Uint8Array,
	defaultName: string,
	filters: { name: string; extensions: string[] }[],
	title = 'Export results',
): Promise<SaveResult> {
	try {
		const { save } = await import('@tauri-apps/plugin-dialog')
		const { writeFile } = await import('@tauri-apps/plugin-fs')
		const path = await save({
			title,
			defaultPath: defaultName,
			filters,
		})
		if (path == null) return { ok: false, cancelled: true }
		await writeFile(path, bytes)
		return { ok: true, path }
	} catch (e) {
		return {
			ok: false,
			cancelled: false,
			error: e instanceof Error ? e.message : String(e),
		}
	}
}

export async function exportPng(
	bytes: Uint8Array,
	fileBase = 'diagram',
): Promise<SaveResult> {
	return saveBytesWithPrompt(
		bytes,
		`${safeFileBase(fileBase)}.png`,
		[{ name: 'PNG', extensions: ['png'] }],
		'Export ER diagram',
	)
}

export async function defaultDownloadsDir(): Promise<string | null> {
	try {
		const { downloadDir } = await import('@tauri-apps/api/path')
		return await downloadDir()
	} catch {
		return null
	}
}

function normalizePickedPath(selected: unknown): string | null {
	if (typeof selected === 'string' && selected.trim()) {
		return selected.trim().replace(/^file:\/\//, '')
	}
	if (Array.isArray(selected)) return normalizePickedPath(selected[0])
	if (selected && typeof selected === 'object') {
		const rec = selected as { path?: unknown }
		if (typeof rec.path === 'string') return normalizePickedPath(rec.path)
	}
	return null
}

export async function pickDataFile(
	formats: string[],
	defaultPath?: string | null,
): Promise<string | null> {
	const { open } = await import('@tauri-apps/plugin-dialog')
	const allowed = (formats ?? []).filter((id) => typeof id === 'string')
	const filters = allowed
		.map((id) => {
			const ext = id === 'jsonl' ? ['jsonl', 'ndjson'] : [id]
			const name =
				id === 'csv'
					? 'CSV'
					: id === 'json'
						? 'JSON'
						: id === 'jsonl'
							? 'JSON Lines'
							: id === 'sql'
								? 'SQL'
								: id.toUpperCase()
			return { name, extensions: ext }
		})
		.concat([{ name: 'All files', extensions: ['*'] }])
	const selected = await open({
		title: 'Import data',
		multiple: false,
		directory: false,
		filters,
		defaultPath: defaultPath || undefined,
	})
	return normalizePickedPath(selected)
}

export function inferFormatFromPath(
	path: string,
	_allowed?: string[],
): TableFileFormat | null {
	const base =
		path
			.trim()
			.replace(/^file:\/\//, '')
			.split(/[/\\]/)
			.pop()
			?.split('?')[0] ?? ''
	const dot = base.lastIndexOf('.')
	const ext = (dot >= 0 ? base.slice(dot + 1) : '').toLowerCase()
	if (ext === 'ndjson' || ext === 'jsonl') return 'jsonl'
	if (ext === 'json' || ext === 'csv' || ext === 'sql') return ext
	return null
}

export async function pickDirectory(
	defaultPath?: string | null,
): Promise<string | null> {
	const { open } = await import('@tauri-apps/plugin-dialog')
	const selected = await open({
		directory: true,
		multiple: false,
		title: 'Choose export folder',
		defaultPath: defaultPath || undefined,
	})
	if (typeof selected === 'string' && selected.trim()) return selected
	return null
}
