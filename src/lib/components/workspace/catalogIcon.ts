import type {
	CatalogObjectKind,
	WorkspaceTabKind,
} from '$lib/types/workspace'

export type CatalogIconName =
	| CatalogObjectKind
	| WorkspaceTabKind
	| 'schema'
	| 'database'
	| 'chevron'

export type CatalogIconTone = 'muted' | 'accent' | 'info' | 'success'

/** Shared kind → glyph. Table tabs use the live object kind (table / view / collection). */
export function iconNameForTab(
	tabKind: WorkspaceTabKind,
	objectKind?: CatalogObjectKind | null,
): CatalogIconName {
	if (tabKind === 'table') return objectKind ?? 'table'
	return tabKind
}

/** Quiet kind color so the same glyph reads the same in the tree and the tab bar. */
export function iconToneForName(
	name: CatalogIconName,
	active = false,
): CatalogIconTone {
	if (active) return 'accent'
	if (name === 'query') return 'accent'
	if (name === 'view' || name === 'model') return 'info'
	if (name === 'table' || name === 'collection') return 'success'
	return 'muted'
}
