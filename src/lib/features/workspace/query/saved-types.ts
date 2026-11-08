export type { SaveQueryInput, SavedQuery } from '#core/contracts'

export type SavedQueryListItem = {
	id: string
	name: string
	sql: string
	favorite: boolean
	createdAt: string
	updatedAt: string
	connectionId?: string
}
