/**
 * Key bindings: parse, match against a keydown, and label for display.
 *
 * A binding is a `+`-joined string such as `Mod+Shift+T`. `Mod` is ⌘ on
 * macOS and Ctrl elsewhere. On macOS a Ctrl chord never matches a `Mod`
 * binding: Ctrl stays with the editor (Ctrl+A, Ctrl+E…).
 *
 * Letters and digits match on `code`, so the keyboard layout does not matter.
 * Symbols match on `key` or `code`.
 */

export type KeyBinding = string

/** A binding that differs per platform. A missing side means no binding there. */
export type PlatformKeys = { mac?: KeyBinding; other?: KeyBinding }

export type KeySpec = KeyBinding | PlatformKeys

export type KeyEventLike = {
	key: string
	code: string
	metaKey: boolean
	ctrlKey: boolean
	altKey: boolean
	shiftKey: boolean
}

type Chord = {
	mod: boolean
	meta: boolean
	ctrl: boolean
	alt: boolean
	shift: boolean
	key: string
}

/** Split on `+`, keeping a trailing `+` as the key (`Mod++`). */
export function keyTokens(binding: KeyBinding): string[] {
	if (binding === '+') return ['+']
	const plusKey = binding.endsWith('++')
	const body = plusKey ? binding.slice(0, -2) : binding
	const parts = body.split('+').filter((part) => part.length > 0)
	return plusKey ? [...parts, '+'] : parts
}

const chordCache = new Map<KeyBinding, Chord>()

function parse(binding: KeyBinding): Chord {
	const hit = chordCache.get(binding)
	if (hit) return hit
	const tokens = keyTokens(binding)
	const chord: Chord = {
		mod: false,
		meta: false,
		ctrl: false,
		alt: false,
		shift: false,
		key: tokens.at(-1) ?? '',
	}
	for (const token of tokens.slice(0, -1)) {
		switch (token.toLowerCase()) {
			case 'mod':
				chord.mod = true
				break
			case 'cmd':
			case 'meta':
				chord.meta = true
				break
			case 'ctrl':
				chord.ctrl = true
				break
			case 'alt':
			case 'option':
				chord.alt = true
				break
			case 'shift':
				chord.shift = true
				break
			default:
				throw new Error(`Unknown modifier "${token}" in "${binding}"`)
		}
	}
	chordCache.set(binding, chord)
	return chord
}

/** Symbols: the `key` values and `code` values that count as the symbol. */
const SYMBOLS: Record<string, { keys: string[]; codes: string[] }> = {
	',': { keys: [','], codes: ['Comma'] },
	'.': { keys: ['.'], codes: ['Period'] },
	'/': { keys: ['/'], codes: ['Slash'] },
	'=': { keys: ['=', '+'], codes: ['Equal', 'NumpadAdd'] },
	'+': { keys: ['+', '='], codes: ['Equal', 'NumpadAdd'] },
	'-': { keys: ['-', '_'], codes: ['Minus', 'NumpadSubtract'] },
	'[': { keys: ['['], codes: ['BracketLeft'] },
	']': { keys: [']'], codes: ['BracketRight'] },
}

const NAMED: Record<string, string> = {
	enter: 'Enter',
	escape: 'Escape',
	esc: 'Escape',
	delete: 'Delete',
	backspace: 'Backspace',
	tab: 'Tab',
	space: ' ',
	up: 'ArrowUp',
	down: 'ArrowDown',
	left: 'ArrowLeft',
	right: 'ArrowRight',
}

type KeyHit = 'code' | 'key' | null

function keyHit(event: KeyEventLike, key: string): KeyHit {
	if (/^[a-z]$/i.test(key)) {
		const upper = key.toUpperCase()
		if (event.code === `Key${upper}`) return 'code'
		// Synthetic events may carry no code.
		if (!event.code && event.key.toUpperCase() === upper) return 'code'
		return null
	}
	if (/^[0-9]$/.test(key)) {
		if (event.code === `Digit${key}`) return 'code'
		if (!event.code && event.key === key) return 'code'
		return null
	}
	const symbol = SYMBOLS[key]
	if (symbol) {
		if (symbol.codes.includes(event.code)) return 'code'
		if (symbol.keys.includes(event.key)) return 'key'
		return null
	}
	const named = NAMED[key.toLowerCase()]
	if (named) return event.key === named ? 'code' : null
	// F1…F12 and anything else: compare the key name.
	return event.key.toLowerCase() === key.toLowerCase() ? 'code' : null
}

/** One binding against one keydown. */
export function matchesBinding(
	event: KeyEventLike,
	binding: KeyBinding,
	mac: boolean,
): boolean {
	const chord = parse(binding)
	const wantMeta = chord.meta || (chord.mod && mac)
	const wantCtrl = chord.ctrl || (chord.mod && !mac)
	if (event.metaKey !== wantMeta) return false
	if (event.ctrlKey !== wantCtrl) return false
	if (event.altKey !== chord.alt) return false
	const hit = keyHit(event, chord.key)
	if (!hit) return false
	// A symbol found by its character may need Shift on some layouts
	// (`/` on a German keyboard, `+` on a US one).
	if (hit === 'key' && !chord.shift) return true
	return event.shiftKey === chord.shift
}

/** The bindings that apply on this platform. */
export function bindingsFor(
	keys: KeySpec | readonly KeySpec[] | undefined,
	mac: boolean,
): KeyBinding[] {
	if (!keys) return []
	const list: readonly KeySpec[] = Array.isArray(keys) ? keys : [keys as KeySpec]
	const out: KeyBinding[] = []
	for (const spec of list) {
		const binding = typeof spec === 'string' ? spec : mac ? spec.mac : spec.other
		if (binding) out.push(binding)
	}
	return out
}

/** Display label for one token. */
export function keyLabel(token: string, mac: boolean): string {
	switch (token.toLowerCase()) {
		case 'mod':
			return mac ? 'Cmd' : 'Ctrl'
		case 'cmd':
		case 'meta':
			return mac ? 'Cmd' : 'Meta'
		case 'ctrl':
			return 'Ctrl'
		case 'shift':
			return 'Shift'
		case 'alt':
		case 'option':
			return mac ? 'Option' : 'Alt'
		case 'enter':
			return 'Enter'
		case 'delete':
			return 'Delete'
		case 'backspace':
			return 'Backspace'
		case 'esc':
		case 'escape':
			return 'Esc'
		case 'tab':
			return 'Tab'
		case 'up':
			return 'Up'
		case 'down':
			return 'Down'
		case 'left':
			return 'Left'
		case 'right':
			return 'Right'
		case '-':
			return '−'
		default:
			return token
	}
}
