import type { ContextMenuItem } from '$core/ui'
import type { ErCtxMenu } from './types'
export type { ErCtxMenu }

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
