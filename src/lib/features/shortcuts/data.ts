/**
 * Keyboard shortcuts shown in the Help dialog, read from the command
 * registry so the list is always what the keys do.
 */
import {
	bindingsFor,
	commands,
	keyLabel,
	keyTokens,
	type CommandSpec,
} from '#core/commands'
import { isMacPlatform } from '#core/util'
// The keymap file alone: the app barrel pulls in rune state.
import { COMMAND_GROUPS } from '#lib/app/commands.js'

export type ShortcutItem = {
	id: string
	label: string
	/** Tokens such as `Mod`, `Shift`, `T`; see `labelForKey`. */
	keys: string[]
	/** Alternate chord shown after “or”. */
	orKeys?: string[]
}

export type ShortcutGroup = {
	id: string
	title: string
	items: ShortcutItem[]
}

export function labelForKey(token: string, mac = isMacPlatform()): string {
	return keyLabel(token, mac)
}

/** One row for a command, or null when it has no keys on this platform. */
export function shortcutItem(spec: CommandSpec, mac = isMacPlatform()): ShortcutItem | null {
	if (spec.display) return { id: spec.id, label: spec.title, keys: [...spec.display] }
	const [first, second] = bindingsFor(spec.keys, mac)
	if (!first) return null
	return {
		id: spec.id,
		label: spec.title,
		keys: keyTokens(first),
		orKeys: second ? keyTokens(second) : undefined,
	}
}

/** Commands with keys, by group, in keymap order. */
export function shortcutGroups(
	mac = isMacPlatform(),
	specs: readonly CommandSpec[] = commands.list(),
): ShortcutGroup[] {
	return COMMAND_GROUPS.map((group) => ({
		id: group.id,
		title: group.title,
		items: specs
			.filter((spec) => spec.group === group.id)
			.map((spec) => shortcutItem(spec, mac))
			.filter((item): item is ShortcutItem => item != null),
	})).filter((group) => group.items.length > 0)
}

export { isMacPlatform }
