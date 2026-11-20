/**
 * The Windows/Linux app menu, drawn in the titlebar. It mirrors the macOS
 * native menu (src-tauri/src/menu.rs) and uses the same item ids, so picking
 * one runs the command bound to it (`commands.runMenu`). Ids in `LOCAL_IDS`
 * have no command: the titlebar runs them itself (edit and window actions).
 */
import type { NativeMenuState } from '#core/ipc'
import type { ContextMenuItem } from '#core/ui'

export type TitleMenu = { label: string; items: ContextMenuItem[] }

/** Item ids the titlebar handles itself. */
export const LOCAL_IDS = [
	'edit_undo',
	'edit_redo',
	'edit_cut',
	'edit_copy',
	'edit_paste',
	'edit_select_all',
	'window_minimize',
	'window_maximize',
	'app_quit',
] as const

/** Shortcuts the webview handles natively, shown on the edit items. */
export const LOCAL_SHORTCUTS: Record<string, string> = {
	edit_undo: 'Ctrl+Z',
	edit_redo: 'Ctrl+Y',
	edit_cut: 'Ctrl+X',
	edit_copy: 'Ctrl+C',
	edit_paste: 'Ctrl+V',
	edit_select_all: 'Ctrl+A',
}

export function appMenu(s: NativeMenuState): TitleMenu[] {
	const home = s.page === 'home'
	const session = !s.hasSession
	const tab = !s.hasTab
	const sel = !s.hasSelection
	const closeAndQuit: ContextMenuItem[] = [
		{ id: 'window_close', label: 'Close Window', separatorBefore: true },
		{ id: 'app_quit', label: 'Exit', separatorBefore: true },
	]
	const zoom: ContextMenuItem[] = [
		{ id: 'view_zoom_in', label: 'Zoom In' },
		{ id: 'view_zoom_out', label: 'Zoom Out' },
		{ id: 'view_zoom_reset', label: 'Actual Size' },
	]
	return [
		{
			label: 'File',
			items: home
				? [{ id: 'app_settings', label: 'Settings…' }, ...closeAndQuit]
				: [
						{
							id: 'workspace_new_query',
							label: 'New Query',
							disabled: session,
						},
						{ id: 'workspace_save_query', label: 'Save', disabled: tab },
						{ id: 'workspace_close_tab', label: 'Close Tab', disabled: tab },
						{
							id: 'workspace_reopen_tab',
							label: 'Reopen Closed Tab',
							disabled: !s.hasClosedTab,
						},
						{ id: 'app_settings', label: 'Settings…', separatorBefore: true },
						...closeAndQuit,
					],
		},
		{
			label: 'Connections',
			items: home
				? [
						{ id: 'home_new_connection', label: 'New Connection' },
						{ id: 'home_import_url', label: 'Import Connection URL…' },
						{ id: 'home_save', label: 'Save Connection', disabled: !s.hasForm },
						{
							id: 'home_connect',
							label: 'Connect',
							disabled: sel,
							separatorBefore: true,
						},
						{ id: 'home_edit', label: 'Edit Connection', disabled: sel },
						{
							id: 'home_duplicate',
							label: 'Duplicate Connection',
							disabled: sel,
						},
						{
							id: 'home_copy_url',
							label: 'Copy Connection URL',
							disabled: sel,
						},
						{ id: 'home_delete', label: 'Delete Connection', disabled: sel },
						{
							id: 'home_refresh',
							label: 'Refresh Connections',
							separatorBefore: true,
						},
					]
				: [
						{ id: 'workspace_open_connection', label: 'Open Connection…' },
						{
							id: 'workspace_connection_details',
							label: 'Connection Details',
							disabled: session,
						},
						{
							id: 'workspace_copy_url',
							label: 'Copy Connection URL',
							disabled: session,
						},
						{
							id: 'workspace_refresh_catalog',
							label: 'Refresh Catalog',
							disabled: session,
						},
						{
							id: 'workspace_disconnect',
							label: 'Disconnect',
							disabled: session,
							separatorBefore: true,
						},
					],
		},
		{
			label: 'Edit',
			items: [
				{ id: 'edit_undo', label: 'Undo' },
				{ id: 'edit_redo', label: 'Redo' },
				{ id: 'edit_cut', label: 'Cut', separatorBefore: true },
				{ id: 'edit_copy', label: 'Copy' },
				{ id: 'edit_paste', label: 'Paste' },
				{ id: 'edit_select_all', label: 'Select All' },
			],
		},
		{
			label: 'View',
			items: home
				? zoom
				: [
						{ id: 'workspace_view_explorer', label: 'Explorer' },
						{ id: 'workspace_view_saved', label: 'Saved Queries' },
						{ id: 'workspace_view_history', label: 'History' },
						{
							id: 'workspace_go_home',
							label: 'Go to Home',
							separatorBefore: true,
						},
						{ ...zoom[0]!, separatorBefore: true },
						...zoom.slice(1),
					],
		},
		{
			label: 'Window',
			items: [
				{ id: 'window_minimize', label: 'Minimize' },
				{ id: 'window_maximize', label: 'Maximize' },
			],
		},
		{
			label: 'Help',
			items: [
				{ id: 'help_shortcuts', label: 'Keyboard Shortcuts' },
				{
					id: 'app_check_updates',
					label: 'Check for Updates…',
					separatorBefore: true,
				},
				{ id: 'app_about', label: 'About Mystique' },
			],
		},
	]
}
