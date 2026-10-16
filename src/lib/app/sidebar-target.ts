/** Which sidebar ⌘/Ctrl+B collapses. */

export type SidebarTarget = 'session-rail' | 'home-sidebar'

export function sidebarTarget(pathname: string): SidebarTarget {
	return pathname === '/workspace' || pathname.startsWith('/workspace/')
		? 'session-rail'
		: 'home-sidebar'
}
