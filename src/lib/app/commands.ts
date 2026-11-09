/**
 * The keymap: every app command's id, title, keys and native menu items.
 *
 * Features register the handlers (`commands.register`) while they are
 * mounted; the root layout declares this list once, and the shortcuts view
 * reads it back from the registry. A command with a `menu` id is run by the
 * menu accelerator in the desktop app, so its `keys` must match menu.rs
 * (checked by scripts/workspace/commands.test.ts).
 *
 * Rows without a registered handler document keys a component handles itself
 * (CodeMirror keymap, grid navigation).
 */
import { defineCommand, type CommandSpec } from '#core/commands'

export const COMMAND_GROUPS = [
	{ id: 'app', title: 'App' },
	{ id: 'home', title: 'Home' },
	{ id: 'workspace', title: 'Workspace' },
	{ id: 'query', title: 'Query editor' },
	{ id: 'grid', title: 'Table' },
	{ id: 'model', title: 'Model' },
] as const

const FOCUS_TAB_KEYS = [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => `Mod+${n}`)

/** Outside text fields and code editors. */
const NOT_TYPING = { editor: false, input: false } as const

export const cmd = {
	// App
	settings: defineCommand<{ section?: string }>({
		id: 'app.settings',
		title: 'Open settings',
		group: 'app',
		keys: 'Mod+,',
		menu: 'app_settings',
	}),
	shortcuts: defineCommand({
		id: 'app.shortcuts',
		title: 'Keyboard shortcuts',
		group: 'app',
		menu: ['app_shortcuts', 'help_shortcuts'],
	}),
	reset: defineCommand({
		id: 'app.reset',
		title: 'Close connections and return home',
		group: 'app',
		keys: 'Mod+Shift+R',
	}),
	zoomIn: defineCommand({
		id: 'app.zoom-in',
		title: 'Increase UI scale',
		group: 'app',
		keys: 'Mod+=',
		display: ['mod', '+'],
		menu: 'view_zoom_in',
		repeat: true,
	}),
	zoomOut: defineCommand({
		id: 'app.zoom-out',
		title: 'Decrease UI scale',
		group: 'app',
		keys: 'Mod+-',
		menu: 'view_zoom_out',
		repeat: true,
	}),
	zoomReset: defineCommand({
		id: 'app.zoom-reset',
		title: 'Reset UI scale',
		group: 'app',
		keys: 'Mod+0',
		menu: 'view_zoom_reset',
	}),
	quickOpen: defineCommand({
		id: 'app.quick-open',
		title: 'Quick open',
		group: 'workspace',
		keys: 'Mod+K',
		// Toggles: closes Quick Open (itself a dialog) too.
		scope: { dialog: true },
	}),
	toggleSidebar: defineCommand({
		id: 'app.toggle-sidebar',
		title: 'Toggle session rail',
		group: 'workspace',
		keys: 'Mod+B',
	}),

	// Home
	homeBack: defineCommand({
		id: 'home.back-to-new',
		title: 'Back to new connection',
		group: 'home',
		keys: 'Escape',
		scope: NOT_TYPING,
	}),
	homeNewConnection: defineCommand({
		id: 'home.new-connection',
		title: 'New connection',
		group: 'home',
		menu: 'home_new_connection',
	}),
	homeImportUrl: defineCommand({
		id: 'home.import-url',
		title: 'Import connection URL',
		group: 'home',
		menu: 'home_import_url',
	}),
	homeConnect: defineCommand({
		id: 'home.connect',
		title: 'Connect',
		group: 'home',
		menu: 'home_connect',
	}),
	homeEdit: defineCommand({
		id: 'home.edit',
		title: 'Edit connection',
		group: 'home',
		menu: 'home_edit',
	}),
	homeDuplicate: defineCommand({
		id: 'home.duplicate',
		title: 'Duplicate connection',
		group: 'home',
		menu: 'home_duplicate',
	}),
	homeCopyUrl: defineCommand({
		id: 'home.copy-url',
		title: 'Copy connection URL',
		group: 'home',
		menu: 'home_copy_url',
	}),
	homeDelete: defineCommand({
		id: 'home.delete',
		title: 'Delete connection',
		group: 'home',
		menu: 'home_delete',
	}),
	homeRefresh: defineCommand({
		id: 'home.refresh',
		title: 'Refresh connections',
		group: 'home',
		menu: 'home_refresh',
	}),
	/** Open (or switch to) a saved connection. Home and the workspace both handle it. */
	openConnectionById: defineCommand<{ id: string }>({
		id: 'connection.open',
		title: 'Open connection',
	}),

	// Workspace
	newQuery: defineCommand({
		id: 'workspace.new-query',
		title: 'New query tab',
		group: 'workspace',
		keys: 'Mod+T',
		menu: 'workspace_new_query',
	}),
	closeTab: defineCommand({
		id: 'workspace.close-tab',
		title: 'Close dialog, or tab if none is open',
		group: 'workspace',
		keys: 'Mod+W',
		menu: 'workspace_close_tab',
		scope: { dialog: true },
	}),
	reopenTab: defineCommand({
		id: 'workspace.reopen-tab',
		title: 'Reopen closed tab',
		group: 'workspace',
		keys: 'Mod+Shift+T',
		menu: 'workspace_reopen_tab',
	}),
	focusFilter: defineCommand({
		id: 'workspace.focus-filter',
		title: 'Focus object filter (outside the editor)',
		group: 'workspace',
		keys: 'Mod+L',
		// ⌘L is select-line in the editor.
		scope: NOT_TYPING,
	}),
	focusTab: defineCommand<{ index: number }>({
		id: 'workspace.focus-tab',
		title: 'Focus workspace tab 1–9',
		group: 'workspace',
		keys: FOCUS_TAB_KEYS,
		display: ['mod', '1'],
	}),
	prevConnection: defineCommand({
		id: 'workspace.prev-connection',
		title: 'Previous connection',
		group: 'workspace',
		keys: 'Mod+Shift+[',
	}),
	nextConnection: defineCommand({
		id: 'workspace.next-connection',
		title: 'Next connection',
		group: 'workspace',
		keys: 'Mod+Shift+]',
	}),
	save: defineCommand({
		id: 'workspace.save',
		title: 'Save query or table',
		group: 'workspace',
		keys: 'Mod+S',
		menu: 'workspace_save_query',
	}),
	openConnection: defineCommand({
		id: 'workspace.open-connection',
		title: 'Open another connection',
		group: 'workspace',
		keys: 'Mod+O',
		menu: 'workspace_open_connection',
	}),
	cancelLoad: defineCommand({
		id: 'workspace.cancel-load',
		title: 'Stop loading the table',
		group: 'workspace',
		keys: 'Escape',
		scope: NOT_TYPING,
	}),
	connectionDetails: defineCommand({
		id: 'workspace.connection-details',
		title: 'Connection details',
		group: 'workspace',
		menu: 'workspace_connection_details',
	}),
	copyConnectionUrl: defineCommand({
		id: 'workspace.copy-url',
		title: 'Copy connection URL',
		group: 'workspace',
		menu: 'workspace_copy_url',
	}),
	refreshCatalog: defineCommand({
		id: 'workspace.refresh-catalog',
		title: 'Refresh catalog',
		group: 'workspace',
		menu: 'workspace_refresh_catalog',
	}),
	disconnect: defineCommand({
		id: 'workspace.disconnect',
		title: 'Disconnect',
		group: 'workspace',
		menu: 'workspace_disconnect',
	}),
	goHome: defineCommand({
		id: 'workspace.go-home',
		title: 'Go to Home',
		group: 'workspace',
		menu: 'workspace_go_home',
	}),
	viewExplorer: defineCommand({
		id: 'workspace.view-explorer',
		title: 'Explorer',
		group: 'workspace',
		menu: 'workspace_view_explorer',
	}),
	viewSaved: defineCommand({
		id: 'workspace.view-saved',
		title: 'Saved queries',
		group: 'workspace',
		menu: 'workspace_view_saved',
	}),
	viewHistory: defineCommand({
		id: 'workspace.view-history',
		title: 'History',
		group: 'workspace',
		menu: 'workspace_view_history',
	}),

	// Query editor. Inside the editor its keymap runs these; the registry
	// handles the chord when focus is elsewhere in the query tab.
	run: defineCommand({
		id: 'query.run',
		title: 'Run statement at cursor',
		group: 'query',
		keys: 'Mod+Enter',
		scope: { editor: false },
	}),
	runAll: defineCommand({
		id: 'query.run-all',
		title: 'Run all statements',
		group: 'query',
		keys: 'Mod+Shift+Enter',
		scope: { editor: false },
	}),
	cancelRun: defineCommand({
		id: 'query.cancel',
		title: 'Cancel the running query',
		group: 'query',
		keys: 'Escape',
	}),
	find: defineCommand({
		id: 'query.find',
		title: 'Find in query',
		group: 'query',
		keys: 'Mod+F',
	}),
	findNext: defineCommand({
		id: 'query.find-next',
		title: 'Find next',
		group: 'query',
		keys: 'Mod+G',
	}),
	format: defineCommand({
		id: 'query.format',
		title: 'Format query',
		group: 'query',
		keys: 'Mod+Shift+F',
	}),
	// Ctrl+Alt on every platform: Cmd+Alt+arrows add cursors in the editor,
	// and Ctrl+Alt+letter is AltGr on Windows.
	prevStatement: defineCommand({
		id: 'query.prev-statement',
		title: 'Go to previous statement',
		group: 'query',
		keys: 'Ctrl+Alt+Up',
		repeat: true,
	}),
	nextStatement: defineCommand({
		id: 'query.next-statement',
		title: 'Go to next statement',
		group: 'query',
		keys: 'Ctrl+Alt+Down',
		repeat: true,
	}),
	selectStatement: defineCommand({
		id: 'query.select-statement',
		title: 'Select statement at cursor',
		group: 'query',
		keys: 'Mod+Shift+Space',
	}),

	// Table
	gridRefresh: defineCommand({
		id: 'grid.refresh',
		title: 'Reload this page of rows (press twice to discard edits)',
		group: 'grid',
		keys: 'Mod+R',
		scope: { editor: false },
	}),
	gridNewRow: defineCommand({
		id: 'grid.new-row',
		title: 'New row',
		group: 'grid',
		keys: 'Mod+N',
		scope: { editor: false },
	}),
	gridDuplicateRow: defineCommand({
		id: 'grid.duplicate-row',
		title: 'Duplicate row',
		group: 'grid',
		keys: 'Mod+D',
		scope: { editor: false },
	}),
	gridUndo: defineCommand({
		id: 'grid.undo',
		title: 'Undo edit',
		group: 'grid',
		keys: 'Mod+Z',
		scope: NOT_TYPING,
		repeat: true,
	}),
	gridRedo: defineCommand({
		id: 'grid.redo',
		title: 'Redo edit',
		group: 'grid',
		keys: 'Mod+Shift+Z',
		scope: NOT_TYPING,
		repeat: true,
	}),
	gridDeleteRow: defineCommand({
		id: 'grid.delete-row',
		title: 'Delete row',
		group: 'grid',
		keys: 'Mod+Delete',
	}),
	gridCopy: defineCommand({ id: 'grid.copy', title: 'Copy cell', group: 'grid', keys: 'Mod+C' }),
	gridPaste: defineCommand({ id: 'grid.paste', title: 'Paste', group: 'grid', keys: 'Mod+V' }),
	gridEdit: defineCommand({
		id: 'grid.edit',
		title: 'Edit cell',
		group: 'grid',
		keys: ['Enter', 'F2'],
	}),
	gridExpand: defineCommand({
		id: 'grid.expand',
		title: 'Open cell editor',
		group: 'grid',
		keys: 'Shift+Enter',
	}),
	gridNull: defineCommand({
		id: 'grid.set-null',
		title: 'Set cell to NULL',
		group: 'grid',
		keys: 'Delete',
	}),
	gridMove: defineCommand({
		id: 'grid.move',
		title: 'Move selection',
		group: 'grid',
		keys: ['Up', 'Down', 'Left', 'Right'],
		display: ['up', 'down', 'left', 'right'],
	}),
	structureFind: defineCommand({
		id: 'grid.find-structure',
		title: 'Filter the structure view',
		group: 'grid',
		keys: ['Mod+F', '/'],
		scope: NOT_TYPING,
	}),

	// Model
	erLayout: defineCommand({
		id: 'er.auto-layout',
		title: 'Auto-layout',
		group: 'model',
		keys: 'Mod+Shift+L',
		scope: NOT_TYPING,
	}),
	erExportPng: defineCommand({
		id: 'er.export-png',
		title: 'Download ERD as PNG',
		group: 'model',
		keys: 'Mod+Shift+E',
		scope: NOT_TYPING,
	}),
	erSelectAll: defineCommand({
		id: 'er.select-all',
		title: 'Select all tables',
		group: 'model',
		keys: 'Mod+A',
		scope: NOT_TYPING,
	}),
	erZoomIn: defineCommand({
		id: 'er.zoom-in',
		title: 'Zoom in',
		group: 'model',
		keys: 'Mod+=',
		display: ['mod', '+'],
		scope: NOT_TYPING,
		repeat: true,
	}),
	erZoomOut: defineCommand({
		id: 'er.zoom-out',
		title: 'Zoom out',
		group: 'model',
		keys: 'Mod+-',
		scope: NOT_TYPING,
		repeat: true,
	}),
	erZoomReset: defineCommand({
		id: 'er.zoom-reset',
		title: 'Reset zoom',
		group: 'model',
		keys: 'Mod+0',
		scope: NOT_TYPING,
	}),
}

/** Every command, in the order the shortcuts view lists them. */
export const KEYMAP: readonly CommandSpec[] = Object.values(cmd) as CommandSpec[]
