import { sidebarTarget } from './sidebar-target'

export type { SidebarTarget } from './sidebar-target'
export { sidebarTarget } from './sidebar-target'

class AppChrome {
	sessionRailOpen = $state(true)
	homeSidebarOpen = $state(true)

	toggle(pathname: string) {
		if (sidebarTarget(pathname) === 'session-rail') {
			this.sessionRailOpen = !this.sessionRailOpen
		} else {
			this.homeSidebarOpen = !this.homeSidebarOpen
		}
	}
}

export const appChrome = new AppChrome()
