import type { ContextMenuItem } from '$lib/components/core'

export type ErCtxMenu = {
	clientX: number
	clientY: number
	kind: 'table' | 'canvas'
	tableId?: string
}

export function erCtxMenuItems(opts: {
	ctx: ErCtxMenu | null
	selectionCount: number
	canPaste: boolean
}): ContextMenuItem[] {
	const { ctx, selectionCount, canPaste } = opts
	if (!ctx) return []
	if (ctx.kind === 'canvas') {
		const items: ContextMenuItem[] = [
			{ id: 'select-all', label: 'Select all', shortcut: 'Ctrl+A' },
			{
				id: 'paste',
				label: 'Paste tables',
				shortcut: 'Ctrl+V',
				disabled: !canPaste,
			},
		]
		if (selectionCount > 0) {
			items.push({
				id: 'delete',
				label:
					selectionCount === 1
						? 'Delete table'
						: `Delete ${selectionCount} tables`,
				shortcut: 'Del',
				danger: true,
				separatorBefore: true,
			})
		}
		return items
	}
	return [
		{
			id: 'copy',
			label: selectionCount > 1 ? `Copy ${selectionCount} tables` : 'Copy table',
			shortcut: 'Ctrl+C',
		},
		{ id: 'paste', label: 'Paste', shortcut: 'Ctrl+V', disabled: !canPaste },
		{
			id: 'delete',
			label:
				selectionCount > 1 ? `Delete ${selectionCount} tables` : 'Delete table',
			shortcut: 'Del',
			danger: true,
			separatorBefore: true,
		},
	]
}
