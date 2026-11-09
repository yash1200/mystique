/** Tool card titles. Unknown tools get a readable form of their name. */

const DONE: Record<string, string> = {
	get_schema_dbml: 'Fetched schema',
	list_objects: 'Listed objects',
	describe_object: 'Described object',
	sample_rows: 'Sampled rows',
	explain_query: 'Explained query',
	infer_fields: 'Inferred fields',
	execute_readonly_query: 'Probed rows',
	execute_query: 'Ran statement',
	list_saved_queries: 'Listed saved queries',
	list_connections: 'Listed connections',
	save_query: 'Saved query',
}

const RUNNING: Record<string, string> = {
	get_schema_dbml: 'Fetching schema',
	list_objects: 'Listing objects',
	describe_object: 'Describing object',
	sample_rows: 'Sampling rows',
	explain_query: 'Explaining query',
	infer_fields: 'Inferring fields',
	execute_readonly_query: 'Probing rows',
	execute_query: 'Running statement',
	list_saved_queries: 'Listing saved queries',
	list_connections: 'Listing connections',
	save_query: 'Saving query',
}

function readable(name: string): string {
	const text = name.replace(/_/g, ' ').trim()
	return text ? text.charAt(0).toUpperCase() + text.slice(1) : 'Tool'
}

export function toolLabel(name?: string | null, running = false): string {
	if (!name) return 'Tool'
	return (running ? RUNNING[name] : DONE[name]) ?? readable(name)
}

/** A short hint from the call's arguments: the object or the statement. */
export function toolTarget(args: unknown): string {
	if (!args || typeof args !== 'object') return ''
	const a = args as Record<string, unknown>
	const name = typeof a.name === 'string' ? a.name : ''
	const schema = typeof a.schema === 'string' ? a.schema : ''
	const collection = typeof a.collection === 'string' ? a.collection : ''
	if (name) return schema ? `${schema}.${name}` : name
	if (collection) return collection
	const query = typeof a.query === 'string' ? a.query : ''
	return query.replace(/\s+/g, ' ').trim().slice(0, 80)
}
