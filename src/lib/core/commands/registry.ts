/**
 * The command registry. A command is an id, a title, optional key bindings
 * and native-menu ids, and handlers that features register while they are
 * mounted. Keys, menu items and code all run commands through here.
 *
 * - One keydown listener (capture phase) for the whole window.
 * - Several handlers may share an id; the newest one whose `when` passes runs,
 *   so a mounted view can take over a chord from the app-wide default.
 * - With a native menu, a command that has a menu item is run by the menu
 *   only. Its accelerator is the binding, and keydown leaves it alone.
 *
 * The registry knows nothing about features.
 */
import { bindingsFor, matchesBinding, type KeySpec } from './keys'
import { allowedIn, focusOf, type Focus, type Scope } from './scope'

export type CommandSource = 'key' | 'menu' | 'call'

export type CommandContext<A = unknown> = {
	source: CommandSource
	/** Set when the command was run from code with arguments. */
	args: A | undefined
	/** The keydown that ran it (source `key`). */
	event: KeyboardEvent | null
	/** Where focus was (source `key`). */
	focus: Focus | null
}

export type CommandSpec<A = unknown> = {
	id: string
	title: string
	/** Shortcuts-view group. */
	group?: string
	/** Alternates in order; the first is the one shown. */
	keys?: KeySpec | readonly KeySpec[]
	/** Tokens the shortcuts view shows instead of the bindings. */
	display?: readonly string[]
	/** Native menu item ids (menu.rs) that run this command. */
	menu?: string | readonly string[]
	scope?: Scope
	/** Run on auto-repeat. Otherwise a held key runs once (later repeats are swallowed). */
	repeat?: boolean
	/** Type carrier for `args`; never set. */
	readonly __args?: A
}

export type CommandHandler<A = unknown> = {
	when?: (ctx: CommandContext<A>) => boolean
	run: (ctx: CommandContext<A>) => unknown
}

/** A spec plus its handler: what a feature registers. */
export type Command<A = unknown> = CommandSpec<A> & CommandHandler<A>

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyCommand = Command<any>

export type RegistryEnv = {
	mac: () => boolean
	/** The desktop app has a native menu; the browser preview does not. */
	nativeMenu: () => boolean
	dialogOpen: () => boolean
}

type Entry = {
	spec: CommandSpec
	when?: (ctx: CommandContext) => boolean
	run: (ctx: CommandContext) => unknown
}

/** Typed identity: keeps `args` attached to the spec. */
export function defineCommand<A = void>(spec: Omit<CommandSpec<A>, '__args'>): CommandSpec<A> {
	return spec as CommandSpec<A>
}

/** A spec with its handler, typed by the spec's `args`. */
export function withHandler<A>(spec: CommandSpec<A>, handler: CommandHandler<A>): Command<A> {
	return { ...spec, ...handler }
}

function menuIds(spec: CommandSpec): readonly string[] {
	if (!spec.menu) return []
	return typeof spec.menu === 'string' ? [spec.menu] : spec.menu
}

export function createCommandRegistry(env: RegistryEnv) {
	/** Declared specs in declaration order. */
	const specs = new Map<string, CommandSpec>()
	/** Registered handlers, oldest first. */
	const entries: Entry[] = []

	function declare(list: readonly CommandSpec[]): () => void {
		const added: string[] = []
		for (const spec of list) {
			if (specs.has(spec.id)) continue
			specs.set(spec.id, spec)
			added.push(spec.id)
		}
		return () => {
			for (const id of added) specs.delete(id)
		}
	}

	/**
	 * Register handlers. Keys, menu ids and scope come from the declared spec
	 * when there is one, so the keymap stays the single source.
	 */
	function register(command: AnyCommand | readonly AnyCommand[]): () => void {
		const list = (Array.isArray(command) ? command : [command]) as AnyCommand[]
		const mine: Entry[] = []
		const declaredHere: string[] = []
		for (const cmd of list) {
			if (!specs.has(cmd.id)) {
				const { when: _when, run: _run, ...spec } = cmd
				specs.set(cmd.id, spec)
				declaredHere.push(cmd.id)
			}
			const entry: Entry = { spec: specs.get(cmd.id)!, when: cmd.when, run: cmd.run }
			entries.push(entry)
			mine.push(entry)
		}
		return () => {
			for (const entry of mine) {
				const i = entries.indexOf(entry)
				if (i >= 0) entries.splice(i, 1)
			}
			for (const id of declaredHere) {
				if (!entries.some((e) => e.spec.id === id)) specs.delete(id)
			}
		}
	}

	function invoke(entry: Entry, ctx: CommandContext): void {
		try {
			const out = entry.run(ctx)
			if (out instanceof Promise) {
				out.catch((err: unknown) => console.error(`[commands] ${entry.spec.id}:`, err))
			}
		} catch (err) {
			console.error(`[commands] ${entry.spec.id}:`, err)
		}
	}

	/** The newest handler for `pick` whose `when` passes. */
	function first(pick: (entry: Entry) => boolean, ctx: CommandContext): Entry | null {
		for (let i = entries.length - 1; i >= 0; i--) {
			const entry = entries[i]!
			if (!pick(entry)) continue
			if (entry.when && !entry.when(ctx)) continue
			return entry
		}
		return null
	}

	/** Run from code. Focus scope does not apply; `when` does. */
	function run<A>(command: string | CommandSpec<A>, args?: A): boolean {
		const id = typeof command === 'string' ? command : command.id
		const ctx: CommandContext = { source: 'call', args, event: null, focus: null }
		const entry = first((e) => e.spec.id === id, ctx)
		if (!entry) return false
		invoke(entry, ctx)
		return true
	}

	/** True when `run` would find a handler. */
	function available(command: string | CommandSpec, args?: unknown): boolean {
		const id = typeof command === 'string' ? command : command.id
		const ctx: CommandContext = { source: 'call', args, event: null, focus: null }
		return first((e) => e.spec.id === id, ctx) != null
	}

	/** A native menu item was chosen (or its accelerator pressed). */
	function runMenu(menuId: string): boolean {
		const ctx: CommandContext = { source: 'menu', args: undefined, event: null, focus: null }
		const entry = first((e) => menuIds(e.spec).includes(menuId), ctx)
		if (!entry) return false
		invoke(entry, ctx)
		return true
	}

	/** True when keydown handles this spec (the menu owns it otherwise). */
	function keyOwned(spec: CommandSpec): boolean {
		if (!spec.keys) return false
		return !(spec.menu && env.nativeMenu())
	}

	/** Dispatch one keydown. True when a command took it. */
	function handleKey(event: KeyboardEvent): boolean {
		if (event.isComposing || event.defaultPrevented) return false
		const mac = env.mac()
		let focus: Focus | null = null
		for (let i = entries.length - 1; i >= 0; i--) {
			const entry = entries[i]!
			const { spec } = entry
			if (!keyOwned(spec)) continue
			if (!bindingsFor(spec.keys, mac).some((b) => matchesBinding(event, b, mac))) {
				continue
			}
			focus ??= focusOf(event.target, env.dialogOpen())
			if (!allowedIn(spec.scope, focus)) continue
			const ctx: CommandContext = { source: 'key', args: undefined, event, focus }
			if (entry.when && !entry.when(ctx)) continue
			event.preventDefault()
			event.stopImmediatePropagation()
			if (event.repeat && !spec.repeat) return true
			invoke(entry, ctx)
			return true
		}
		return false
	}

	/** The single window keydown listener. */
	function mount(target: Window = window): () => void {
		const onKey = (event: KeyboardEvent) => {
			handleKey(event)
		}
		target.addEventListener('keydown', onKey, true)
		return () => target.removeEventListener('keydown', onKey, true)
	}

	function list(): CommandSpec[] {
		return [...specs.values()]
	}

	function get(id: string): CommandSpec | undefined {
		return specs.get(id)
	}

	return { declare, register, run, available, runMenu, handleKey, mount, list, get }
}

export type CommandRegistry = ReturnType<typeof createCommandRegistry>
