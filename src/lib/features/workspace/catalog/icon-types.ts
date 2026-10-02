import type { CatalogObjectKind } from '#core/contracts'
import type { WorkspaceTabKind } from '#features/workspace/shell'

export type CatalogIconName =
	| CatalogObjectKind
	| WorkspaceTabKind
	| 'schema'
	| 'database'
	| 'chevron'

export type CatalogIconTone = 'muted' | 'accent' | 'info' | 'success'
