/**
 * Product vs runtime — keep in sync with
 * `crates/mystique-connector-api/src/products.rs`.
 *
 * Product = picker / saved `engine` column. Runtime = plugin crate.
 * `onPicker` is a Home card; aliases (`postgresql`, `pg`, `mongo`) stay off.
 */
export interface EngineProduct {
	id: string
	runtime: string
	label: string
	mark: string
	defaultPort: number
	description: string
	onPicker: boolean
	supportDepth: 'operate' | 'browse' | 'connect'
}

const ENGINE_PRODUCTS: readonly EngineProduct[] = [
	{
		id: 'postgres',
		runtime: 'postgres',
		label: 'PostgreSQL',
		mark: 'Pg',
		defaultPort: 5432,
		description:
			'Relational studio — schemas, tables, SQL, and classic ER Model view from foreign keys.',
		onPicker: true,
		supportDepth: 'operate',
	},
	{
		id: 'postgresql',
		runtime: 'postgres',
		label: 'PostgreSQL',
		mark: 'Pg',
		defaultPort: 5432,
		description: 'PostgreSQL (postgresql alias).',
		onPicker: false,
		supportDepth: 'operate',
	},
	{
		id: 'pg',
		runtime: 'postgres',
		label: 'PostgreSQL',
		mark: 'Pg',
		defaultPort: 5432,
		description: 'PostgreSQL (pg alias).',
		onPicker: false,
		supportDepth: 'operate',
	},
	{
		id: 'cockroachdb',
		runtime: 'postgres',
		label: 'CockroachDB',
		mark: 'CR',
		defaultPort: 26257,
		description: 'Distributed SQL — Postgres wire protocol.',
		onPicker: true,
		supportDepth: 'operate',
	},
	{
		id: 'mysql',
		runtime: 'mysql',
		label: 'MySQL',
		mark: 'My',
		defaultPort: 3306,
		description:
			'MySQL — databases, tables, SQL query space, and relational Model view.',
		onPicker: true,
		supportDepth: 'operate',
	},
	{
		id: 'mariadb',
		runtime: 'mysql',
		label: 'MariaDB',
		mark: 'Ma',
		defaultPort: 3306,
		description: 'MariaDB — MySQL wire protocol.',
		onPicker: true,
		supportDepth: 'operate',
	},
	{
		id: 'tidb',
		runtime: 'mysql',
		label: 'TiDB',
		mark: 'Ti',
		defaultPort: 4000,
		description: 'TiDB — MySQL wire protocol.',
		onPicker: true,
		supportDepth: 'operate',
	},
	{
		id: 'sqlite',
		runtime: 'sqlite',
		label: 'SQLite',
		mark: 'Lite',
		defaultPort: 0,
		description: 'Local file database.',
		onPicker: true,
		supportDepth: 'operate',
	},
	{
		id: 'mongodb',
		runtime: 'mongodb',
		label: 'MongoDB',
		mark: 'Mo',
		defaultPort: 27017,
		description:
			'Document studio — databases, collections, find/aggregation, and collection Model map.',
		onPicker: true,
		supportDepth: 'operate',
	},
	{
		id: 'mongo',
		runtime: 'mongodb',
		label: 'MongoDB',
		mark: 'Mo',
		defaultPort: 27017,
		description: 'MongoDB (mongo alias).',
		onPicker: false,
		supportDepth: 'operate',
	},
	{
		id: 'documentdb',
		runtime: 'mongodb',
		label: 'DocumentDB',
		mark: 'Doc',
		defaultPort: 27017,
		description: 'Amazon DocumentDB — MongoDB wire protocol.',
		onPicker: true,
		supportDepth: 'operate',
	},
]

export function productById(id: string): EngineProduct | undefined {
	const key = String(id).trim().toLowerCase()
	return ENGINE_PRODUCTS.find((p) => p.id === key)
}

/** Home “choose database” cards — aliases are resolve-only. */
export function pickerProducts(): EngineProduct[] {
	return ENGINE_PRODUCTS.filter((p) => p.onPicker)
}

export function runtimeFor(id: string): string | undefined {
	return productById(id)?.runtime
}
