import arrowUp from './arrow-up.svg?raw'
import bookmarkFilled from './bookmark-filled.svg?raw'
import bookmark from './bookmark.svg?raw'
import chat from './chat.svg?raw'
import check from './check.svg?raw'
import chevronDown from './chevron-down.svg?raw'
import chevronLeft from './chevron-left.svg?raw'
import chevronRight from './chevron-right.svg?raw'
import close from './close.svg?raw'
import codeSlash from './code-slash.svg?raw'
import code from './code.svg?raw'
import collapseAll from './collapse-all.svg?raw'
import collection from './collection.svg?raw'
import copy from './copy.svg?raw'
import database from './database.svg?raw'
import download from './download.svg?raw'
import expandAll from './expand-all.svg?raw'
import external from './external.svg?raw'
import funnel from './funnel.svg?raw'
import grid from './grid.svg?raw'
import help from './help.svg?raw'
import history from './history.svg?raw'
import info from './info.svg?raw'
import metrics from './metrics.svg?raw'
import model from './model.svg?raw'
import plus from './plus.svg?raw'
import refresh from './refresh.svg?raw'
import schema from './schema.svg?raw'
import search from './search.svg?raw'
import settings from './settings.svg?raw'
import sortAsc from './sort-asc.svg?raw'
import sortDesc from './sort-desc.svg?raw'
import sortNone from './sort-none.svg?raw'
import sort from './sort.svg?raw'
import starFilled from './star-filled.svg?raw'
import star from './star.svg?raw'
import structure from './structure.svg?raw'
import table from './table.svg?raw'
import trash from './trash.svg?raw'
import view from './view.svg?raw'

const files = {
	plus,
	close,
	'arrow-up': arrowUp,
	trash,
	refresh,
	help,
	info,
	metrics,
	sort,
	star,
	'star-filled': starFilled,
	bookmark,
	'bookmark-filled': bookmarkFilled,
	chat,
	history,
	funnel,
	code,
	'code-slash': codeSlash,
	search,
	'chevron-left': chevronLeft,
	'chevron-right': chevronRight,
	'chevron-down': chevronDown,
	check,
	'sort-asc': sortAsc,
	'sort-desc': sortDesc,
	'sort-none': sortNone,
	external,
	copy,
	download,
	settings,
	grid,
	'collapse-all': collapseAll,
	'expand-all': expandAll,
	table,
	view,
	collection,
	query: code,
	model,
	structure,
	schema,
	database,
} as const

export type IconName = keyof typeof files

function svgInner(raw: string): string {
	const match = raw.match(/<svg[^>]*>([\s\S]*)<\/svg>/i)
	return (match?.[1] ?? raw).trim()
}

export const iconInner: Record<IconName, string> = Object.fromEntries(
	(Object.keys(files) as IconName[]).map((name) => [
		name,
		svgInner(files[name]),
	]),
) as Record<IconName, string>
