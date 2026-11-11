import { objectKindSpec } from '#engines'
import { matchTerms, type MatchTier } from './score'

export type QuickOpenKind =
	| 'connection'
	| 'catalog'
	| 'saved'
	| 'history'
	| 'command'

export type QuickOpenCommand =
	| 'new-query'
	| 'metrics'
	| 'toggle-rail'
	| 'focus-filter'
	| 'prev-tab'
	| 'next-tab'

export type QuickOpenItem = {
	id: string
	kind: QuickOpenKind
	title: string
	detail: string
	terms: string[]
	onCurrent: boolean
	showWhenIdle: boolean
	connectionId: string | null
	colorId: string | null
	mark: string
	tableId?: string
	objectKind?: string
	savedId?: string
	body?: string
	command?: QuickOpenCommand
	/** The connection the item belongs to, for its group heading. */
	connectionName?: string
}

export type QuickOpenProfile = {
	id: string
	name: string
	engine: string
	colorId: string
	environment: string
	mark: string
	engineLabel: string
}

export type QuickOpenTable = {
	id: string
	name: string
	schema: string
	database: string
	kind: string
}

export type QuickOpenCatalog = {
	connectionId: string
	name: string
	colorId: string
	mark: string
	tables: QuickOpenTable[]
}

export type QuickOpenSaved = {
	id: string
	connectionId: string
	name: string
	body: string
}

export type QuickOpenHistory = {
	id: string
	connectionId: string | null
	title: string
	body: string
	connectionName: string
}

export type QuickOpenSource = {
	activeConnectionId: string | null
	profiles: QuickOpenProfile[]
	catalogs: QuickOpenCatalog[]
	saved: QuickOpenSaved[]
	history: QuickOpenHistory[]
	inWorkspace: boolean
	supportsMetrics: boolean
}

const IDLE_CONNECTIONS = 12
const IDLE_TABLES = 12
const IDLE_SAVED = 8
const IDLE_HISTORY = 5

export function collectQuickOpenItems(
	source: QuickOpenSource,
): QuickOpenItem[] {
	const active = source.activeConnectionId
	const items: QuickOpenItem[] = []
	if (source.inWorkspace && active) {
		items.push(
			command('new-query', 'New query', '⌘/Ctrl+T', true),
			command('prev-tab', 'Previous connection', '⌘/Ctrl+Shift+[', true),
			command('next-tab', 'Next connection', '⌘/Ctrl+Shift+]', true),
			command('focus-filter', 'Focus object filter', '⌘/Ctrl+L', true),
			command('toggle-rail', 'Toggle session rail', '⌘/Ctrl+B', true),
		)
		if (source.supportsMetrics) {
			items.push(
				command('metrics', 'Server metrics', 'Open the metrics tab', true),
			)
		}
	}

	const profiles = [...source.profiles].sort((a, b) => {
		const aCurrent = a.id === active ? 0 : 1
		const bCurrent = b.id === active ? 0 : 1
		if (aCurrent !== bCurrent) return aCurrent - bCurrent
		return a.name.localeCompare(b.name)
	})
	profiles.forEach((profile, index) => {
		const onCurrent = profile.id === active
		items.push({
			id: `connection:${profile.id}`,
			kind: 'connection',
			title: profile.name,
			detail: `${profile.engineLabel} · ${profile.environment}`,
			terms: [profile.name, profile.engineLabel, profile.engine],
			onCurrent,
			showWhenIdle: index < IDLE_CONNECTIONS,
			connectionId: profile.id,
			colorId: profile.colorId,
			mark: profile.mark,
		})
	})

	let idleTables = 0
	for (const catalog of source.catalogs) {
		const onCurrent = catalog.connectionId === active
		for (const table of catalog.tables) {
			const qualified = [table.database, table.schema, table.name]
				.filter((part) => part.length > 0)
				.join('.')
			const showWhenIdle = onCurrent && idleTables < IDLE_TABLES
			if (showWhenIdle) idleTables += 1
			items.push({
				id: `catalog:${catalog.connectionId}:${table.id}`,
				kind: 'catalog',
				title: table.name,
				detail: [
					catalog.name,
					table.database,
					table.schema,
					kindWord(table.kind),
				]
					.filter((part) => part.length > 0)
					.join(' · '),
				terms: [table.name, qualified, table.database, table.schema].filter(
					(part) => part.length > 0,
				),
				onCurrent,
				showWhenIdle,
				connectionId: catalog.connectionId,
				colorId: catalog.colorId,
				mark: catalog.mark,
				connectionName: catalog.name,
				tableId: table.id,
				objectKind: table.kind,
			})
		}
	}

	source.saved.slice(0, IDLE_SAVED).forEach((saved) => {
		items.push(savedItem(source, saved, saved.connectionId === active, true))
	})
	for (const saved of source.saved.slice(IDLE_SAVED)) {
		items.push(savedItem(source, saved, saved.connectionId === active, false))
	}

	source.history.slice(0, IDLE_HISTORY).forEach((entry) => {
		items.push(
			historyItem(
				source,
				entry,
				entry.connectionId == null || entry.connectionId === active,
				true,
			),
		)
	})
	for (const entry of source.history.slice(IDLE_HISTORY)) {
		items.push(
			historyItem(
				source,
				entry,
				entry.connectionId == null || entry.connectionId === active,
				false,
			),
		)
	}
	return items
}

export function rankQuickOpenItems(
	items: QuickOpenItem[],
	query: string,
	limit: number,
): QuickOpenItem[] {
	const trimmed = query.trim()
	if (!trimmed) return items.filter((item) => item.showWhenIdle).slice(0, limit)
	const ranked: { item: QuickOpenItem; tier: MatchTier }[] = []
	for (const item of items) {
		const tier = matchTerms(trimmed, item.terms)
		if (tier == null) continue
		ranked.push({ item, tier })
	}
	ranked.sort((a, b) => {
		if (a.item.onCurrent !== b.item.onCurrent) return a.item.onCurrent ? -1 : 1
		if (a.tier !== b.tier) return a.tier - b.tier
		const byTitle = a.item.title.localeCompare(b.item.title)
		if (byTitle !== 0) return byTitle
		return a.item.id.localeCompare(b.item.id)
	})
	return ranked.slice(0, limit).map((row) => row.item)
}

export type QuickOpenGroup = {
	id: string
	label: string
	colorId: string | null
	items: QuickOpenItem[]
}

/** Results per connection while searching, so one big catalog cannot fill the list. */
export const QUICK_OPEN_PER_GROUP = 10

/**
 * Ranked items under headings: commands, connections, then one group per
 * connection (its tables, saved queries and history). Groups keep the order
 * of their best item, and items keep their rank inside a group.
 */
export function groupQuickOpenItems(
	ranked: QuickOpenItem[],
	perGroup = Number.POSITIVE_INFINITY,
	total = Number.POSITIVE_INFINITY,
): QuickOpenGroup[] {
	const groups = new Map<string, QuickOpenGroup>()
	let kept = 0
	for (const item of ranked) {
		if (kept >= total) break
		const id = groupIdOf(item)
		let group = groups.get(id)
		if (!group) {
			group = { id, label: groupLabelOf(item), colorId: groupColorOf(item), items: [] }
			groups.set(id, group)
		}
		if (group.items.length >= perGroup) continue
		group.items.push(item)
		kept += 1
	}
	return [...groups.values()].filter((group) => group.items.length > 0)
}

function groupIdOf(item: QuickOpenItem): string {
	if (item.kind === 'command') return 'commands'
	if (item.kind === 'connection') return 'connections'
	return item.connectionId ? `connection:${item.connectionId}` : 'history'
}

function groupLabelOf(item: QuickOpenItem): string {
	if (item.kind === 'command') return 'Commands'
	if (item.kind === 'connection') return 'Connections'
	if (!item.connectionId) return 'History'
	return item.connectionName?.trim() || 'Connection'
}

function groupColorOf(item: QuickOpenItem): string | null {
	if (item.kind === 'command' || item.kind === 'connection') return null
	return item.colorId
}

export function quickOpenKindLabel(item: QuickOpenItem): string {
	if (item.kind === 'catalog') return kindWord(item.objectKind ?? 'table')
	if (item.kind === 'connection') return 'Connection'
	if (item.kind === 'saved') return 'Saved query'
	if (item.kind === 'history') return 'History'
	return 'Command'
}

function command(
	commandId: QuickOpenCommand,
	title: string,
	detail: string,
	onCurrent: boolean,
): QuickOpenItem {
	return {
		id: `command:${commandId}`,
		kind: 'command',
		title,
		detail,
		terms: [title],
		onCurrent,
		showWhenIdle: true,
		connectionId: null,
		colorId: null,
		mark: '',
		command: commandId,
	}
}

function savedItem(
	source: QuickOpenSource,
	saved: QuickOpenSaved,
	onCurrent: boolean,
	showWhenIdle: boolean,
): QuickOpenItem {
	const profile = profileFor(source, saved.connectionId)
	return {
		id: `saved:${saved.id}`,
		kind: 'saved',
		title: saved.name,
		detail: 'Saved query',
		terms: [saved.name],
		onCurrent,
		showWhenIdle,
		connectionId: saved.connectionId,
		colorId: profile?.colorId ?? null,
		mark: profile?.mark ?? '',
		connectionName: profile?.name,
		savedId: saved.id,
		body: saved.body,
	}
}

function historyItem(
	source: QuickOpenSource,
	entry: QuickOpenHistory,
	onCurrent: boolean,
	showWhenIdle: boolean,
): QuickOpenItem {
	const where = entry.connectionName.trim()
	const profile = profileFor(source, entry.connectionId)
	return {
		id: `history:${entry.id}`,
		kind: 'history',
		title: entry.title,
		detail: where.length > 0 ? `History · ${where}` : 'History',
		terms: [entry.title, entry.body],
		onCurrent,
		showWhenIdle,
		connectionId: entry.connectionId,
		colorId: profile?.colorId ?? null,
		mark: profile?.mark ?? '',
		connectionName: profile?.name ?? (where || undefined),
		body: entry.body,
	}
}

function profileFor(source: QuickOpenSource, id: string | null) {
	if (!id) return null
	return source.profiles.find((profile) => profile.id === id) ?? null
}

function kindWord(kind: string): string {
	const word = objectKindSpec(null, kind).singular
	return word.charAt(0).toUpperCase() + word.slice(1)
}
