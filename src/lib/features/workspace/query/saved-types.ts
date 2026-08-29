export type SavedQuery = {
	id: string
	connectionId: string
	name: string
	body: string
	language?: string | null
	sortOrder: number
	favorite: boolean
	createdAt: string
	updatedAt: string
}

export type SaveQueryInput = {
	id?: string | null
	connectionId: string
	name: string
	body: string
	language?: string | null
	sortOrder?: number
	favorite?: boolean
}

export type SavedQueryListItem = {
	id: string
	name: string
	sql: string
	favorite: boolean
	createdAt: string
	updatedAt: string
	connectionId?: string
}
