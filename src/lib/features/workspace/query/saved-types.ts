export type SavedQuery = {
	id: string
	connectionId: string
	name: string
	body: string
	language?: string | null
	sortOrder: number
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
}

export type SavedQueryListItem = {
	id: string
	name: string
	sql: string
	updatedAt: string
	connectionId?: string
}
