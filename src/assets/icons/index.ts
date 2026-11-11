import alertCircle from './alert-circle.svg?raw'
import arrowDown from './arrow-down.svg?raw'
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
import lockOpen from './lock-open.svg?raw'
import lock from './lock.svg?raw'
import metrics from './metrics.svg?raw'
import model from './model.svg?raw'
import plus from './plus.svg?raw'
import plugOff from './plug-off.svg?raw'
import plug from './plug.svg?raw'
import refresh from './refresh.svg?raw'
import schema from './schema.svg?raw'
import search from './search.svg?raw'
import send from './send.svg?raw'
import settings from './settings.svg?raw'
import shield from './shield.svg?raw'
import sortAsc from './sort-asc.svg?raw'
import sortDesc from './sort-desc.svg?raw'
import sortNone from './sort-none.svg?raw'
import sparkle from './sparkle.svg?raw'
import sort from './sort.svg?raw'
import starFilled from './star-filled.svg?raw'
import star from './star.svg?raw'
import structure from './structure.svg?raw'
import table from './table.svg?raw'
import trash from './trash.svg?raw'
import view from './view.svg?raw'
import bolt from './bolt.svg?raw'
import contrast from './contrast.svg?raw'
import format from './format.svg?raw'
import heart from './heart.svg?raw'
import keyboard from './keyboard.svg?raw'
import link from './link.svg?raw'
import pencil from './pencil.svg?raw'
import play from './play.svg?raw'
import shieldAlert from './shield-alert.svg?raw'
import sidebar from './sidebar.svg?raw'
import sliders from './sliders.svg?raw'
import stop from './stop.svg?raw'
import terminal from './terminal.svg?raw'
import upload from './upload.svg?raw'

const files = {
	plus,
	close,
	'alert-circle': alertCircle,
	plug,
	'plug-off': plugOff,
	'arrow-up': arrowUp,
	'arrow-down': arrowDown,
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
	send,
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
	sparkle,
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
	shield,
	lock,
	'lock-open': lockOpen,
	bolt,
	contrast,
	format,
	heart,
	keyboard,
	link,
	pencil,
	play,
	'shield-alert': shieldAlert,
	sidebar,
	sliders,
	stop,
	terminal,
	upload,
} as const

export type IconName = keyof typeof files

function svgInner(raw: string): string {
	const match = raw.match(/<svg[^>]*>([\s\S]*)<\/svg>/i)
	return (match?.[1] ?? raw).trim()
}

/** The drawing grid. Older glyphs are 16px; design-system v1 glyphs are 24px. */
function svgViewBox(raw: string): string {
	return raw.match(/<svg[^>]*\sviewBox="([^"]+)"/i)?.[1] ?? '0 0 16 16'
}

const names = Object.keys(files) as IconName[]

export const iconInner = Object.fromEntries(
	names.map((name) => [name, svgInner(files[name])]),
) as Record<IconName, string>

export const iconViewBox = Object.fromEntries(
	names.map((name) => [name, svgViewBox(files[name])]),
) as Record<IconName, string>
