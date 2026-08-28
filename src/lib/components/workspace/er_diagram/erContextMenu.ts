import type { ContextMenuItem } from '$lib/components/core'

export type ErCtxMenu = {
	clientX: number
	clientY: number
}

export function erCtxMenuItems(opts: {
	canDownload?: boolean
}): ContextMenuItem[] {
	return [
		{
			id: 'download-png',
			label: 'Download PNG',
			icon: 'download',
			shortcut: 'Ctrl+Shift+E',
			disabled: !opts.canDownload,
		},
	]
}
