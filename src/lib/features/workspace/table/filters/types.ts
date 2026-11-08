export type FilterMode = 'ui' | 'code'

export type FilterOp =
	| 'eq'
	| 'neq'
	| 'gt'
	| 'gte'
	| 'lt'
	| 'lte'
	| 'contains'
	| 'starts'
	| 'ends'
	| 'is_null'
	| 'not_null'
	| 'is_empty'
	| 'not_empty'

export type FilterRule = {
	id: string
	column: string
	op: FilterOp
	value: string
}
