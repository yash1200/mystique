import { isMacPlatform } from '$core/util'
/** Keyboard shortcuts shown in the Help modal. Tokens: `mod` `shift` `alt` `enter` `delete` `backspace`. */

export type ShortcutItem = {
	id: string
	label: string
	keys: string[]
	/** Alternate chord shown after “or”. */
	orKeys?: string[]
}

export type ShortcutGroup = {
	id: string
	title: string
	items: ShortcutItem[]
}

export function labelForKey(token: string, mac = isMacPlatform()): string {
	switch (token.toLowerCase()) {
		case 'mod':
			return mac ? 'Cmd' : 'Ctrl'
		case 'shift':
			return 'Shift'
		case 'alt':
			return mac ? 'Option' : 'Alt'
		case 'enter':
			return 'Enter'
		case 'delete':
			return 'Delete'
		case 'backspace':
			return 'Backspace'
		case 'esc':
			return 'Esc'
		case 'tab':
			return 'Tab'
		case 'up':
			return 'Up'
		case 'down':
			return 'Down'
		case 'left':
			return 'Left'
		case 'right':
			return 'Right'
		default:
			return token
	}
}

export const APP_SHORTCUT_GROUPS: ShortcutGroup[] = [
	{
		id: 'app',
		title: 'App',
		items: [
			{
				id: 'reset',
				label: 'Close connections and return home',
				keys: ['mod', 'shift', 'R'],
			},
			{ id: 'zoom-in', label: 'Increase UI scale', keys: ['mod', '+'] },
			{ id: 'zoom-out', label: 'Decrease UI scale', keys: ['mod', '−'] },
			{ id: 'zoom-reset', label: 'Reset UI scale', keys: ['mod', '0'] },
		],
	},
	{
		id: 'home',
		title: 'Home',
		items: [
			{
				id: 'back-to-new',
				label: 'Back to new connection',
				keys: ['esc'],
			},
		],
	},
	{
		id: 'workspace',
		title: 'Workspace',
		items: [
			{ id: 'new-query', label: 'New query tab', keys: ['mod', 'T'] },
			{ id: 'close-tab', label: 'Close tab', keys: ['mod', 'W'] },
			{ id: 'save', label: 'Save query or table', keys: ['mod', 'S'] },
		],
	},
	{
		id: 'query',
		title: 'Query editor',
		items: [
			{ id: 'run', label: 'Run query', keys: ['mod', 'enter'] },
			{ id: 'format', label: 'Format query', keys: ['mod', 'shift', 'F'] },
			{ id: 'save-query', label: 'Save query', keys: ['mod', 'S'] },
		],
	},
	{
		id: 'grid',
		title: 'Table',
		items: [
			{ id: 'save-grid', label: 'Save changes', keys: ['mod', 'S'] },
			{ id: 'new-row', label: 'New row', keys: ['mod', 'N'] },
			{ id: 'dup-row', label: 'Duplicate row', keys: ['mod', 'D'] },
			{ id: 'del-row', label: 'Delete row', keys: ['mod', 'delete'] },
			{ id: 'copy', label: 'Copy cell', keys: ['mod', 'C'] },
			{ id: 'paste', label: 'Paste', keys: ['mod', 'V'] },
			{ id: 'edit', label: 'Edit cell', keys: ['Enter'], orKeys: ['F2'] },
			{ id: 'expand', label: 'Open cell editor', keys: ['shift', 'enter'] },
			{ id: 'null', label: 'Set cell to NULL', keys: ['delete'] },
			{
				id: 'nav',
				label: 'Move selection',
				keys: ['up', 'down', 'left', 'right'],
			},
		],
	},
	{
		id: 'model',
		title: 'Model',
		items: [
			{ id: 'layout', label: 'Auto-layout', keys: ['mod', 'shift', 'L'] },
			{
				id: 'download-png',
				label: 'Download ERD as PNG',
				keys: ['mod', 'shift', 'E'],
			},
			{ id: 'select-all', label: 'Select all tables', keys: ['mod', 'A'] },
		],
	},
]

export { isMacPlatform }
