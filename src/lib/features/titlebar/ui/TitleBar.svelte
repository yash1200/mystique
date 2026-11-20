<script lang="ts">
	import { bindingsFor, commands, keyLabel, keyTokens } from '#core/commands'
	import { menuState } from '#core/ipc'
	import { ContextMenu, type ContextMenuItem } from '#core/ui'
	import { appTitle, cn, isMacPlatform } from '#core/util'
	import { getCurrentWindow } from '@tauri-apps/api/window'
	import { exit } from '@tauri-apps/plugin-process'
	import { onMount } from 'svelte'
	import { appMenu, LOCAL_IDS, LOCAL_SHORTCUTS } from '../menu'

	// macOS keeps its native menu and traffic lights (overlay titlebar); the
	// bar there only holds the title and the drag / double-click-to-zoom area.
	const mac = isMacPlatform()

	const menus = $derived(appMenu(menuState.current))
	let open = $state<{ index: number; x: number; y: number } | null>(null)
	let maximized = $state(false)
	/** The menu an outside press just closed, so that press on its own button does not reopen it. */
	let justClosed = -1

	const local: Record<(typeof LOCAL_IDS)[number], () => unknown> = {
		edit_undo: () => document.execCommand('undo'),
		edit_redo: () => document.execCommand('redo'),
		edit_cut: () => document.execCommand('cut'),
		edit_copy: () => document.execCommand('copy'),
		edit_paste: () =>
			navigator.clipboard
				.readText()
				.then((text) => document.execCommand('insertText', false, text))
				.catch(() => {}),
		edit_select_all: () => document.execCommand('selectAll'),
		window_minimize: () => getCurrentWindow().minimize(),
		window_maximize: () => getCurrentWindow().toggleMaximize(),
		// A real quit: closing the window only hides it while MCP is on.
		app_quit: () => exit(0),
	}

	function shortcut(id: string): string | undefined {
		if (LOCAL_SHORTCUTS[id]) return LOCAL_SHORTCUTS[id]
		const spec = commands.list().find((s) => [s.menu ?? []].flat().includes(id))
		const binding = bindingsFor(spec?.keys, false)[0]
		return (
			binding &&
			keyTokens(binding)
				.map((t) => keyLabel(t, false))
				.join('+')
		)
	}

	function items(index: number): ContextMenuItem[] {
		return menus[index]!.items.map((item) => ({
			...item,
			shortcut: shortcut(item.id),
		}))
	}

	function show(index: number, button: HTMLElement) {
		const box = button.getBoundingClientRect()
		open = { index, x: box.left, y: box.bottom + 2 }
	}

	function close() {
		justClosed = open?.index ?? -1
		open = null
		setTimeout(() => (justClosed = -1))
	}

	function pick(id: string) {
		open = null
		if (commands.runMenu(id)) return
		void local[id as (typeof LOCAL_IDS)[number]]?.()
	}

	onMount(() => {
		if (mac) return
		let win: ReturnType<typeof getCurrentWindow>
		try {
			win = getCurrentWindow()
		} catch {
			return // not in the desktop shell
		}
		const sync = () => void win.isMaximized().then((m) => (maximized = m))
		sync()
		let stop: (() => void) | undefined
		let gone = false
		void win
			.onResized(sync)
			.then((u) => (gone ? u() : (stop = u)))
			.catch(() => {})
		return () => {
			gone = true
			stop?.()
		}
	})
</script>

<header
	class={cn(
		'relative z-70 flex shrink-0 items-center border-b border-line bg-side select-none',
		mac ? 'h-[28px]' : 'h-[32px]',
	)}
	data-tauri-drag-region
>
	<span
		class="pointer-events-none absolute inset-x-37.5 truncate text-center text-caption text-fg-muted"
	>
		{appTitle.current}
	</span>

	{#if !mac}
		<nav
			class="relative flex h-full items-center px-1"
			aria-label="Application menu"
		>
			{#each menus as menu, i (menu.label)}
				<button
					type="button"
					aria-haspopup="menu"
					aria-expanded={open?.index === i}
					class={cn(
						'h-6 cursor-default rounded-[5px] border-none bg-transparent px-2 text-caption text-fg-secondary hover:bg-hover hover:text-fg',
						open?.index === i && 'bg-hover text-fg',
					)}
					onmousedown={(e) => e.preventDefault()}
					onpointerdown={(e) => {
						if (e.button !== 0 || justClosed === i) return
						if (open?.index === i) open = null
						else show(i, e.currentTarget)
					}}
					onpointerenter={(e) => {
						if (open && open.index !== i) show(i, e.currentTarget)
					}}
				>
					{menu.label}
				</button>
			{/each}
		</nav>

		<div class="relative ml-auto flex h-full">
			<button
				type="button"
				class="caption"
				aria-label="Minimize"
				onclick={() => void local.window_minimize()}
			>
				<svg viewBox="0 0 10 10"><path d="M0 5.5h10" /></svg>
			</button>
			<button
				type="button"
				class="caption"
				aria-label={maximized ? 'Restore' : 'Maximize'}
				onclick={() => void local.window_maximize()}
			>
				{#if maximized}
					<svg viewBox="0 0 10 10">
						<path d="M2.5 2.5V.5h7v7h-2" />
						<rect x=".5" y="2.5" width="7" height="7" />
					</svg>
				{:else}
					<svg viewBox="0 0 10 10"
						><rect x=".5" y=".5" width="9" height="9" /></svg
					>
				{/if}
			</button>
			<button
				type="button"
				class="caption close"
				aria-label="Close"
				onclick={() => void getCurrentWindow().close()}
			>
				<svg viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" /></svg>
			</button>
		</div>
	{/if}

	{#if open}
		{#key open.index}
			<ContextMenu
				x={open.x}
				y={open.y}
				items={items(open.index)}
				ariaLabel={menus[open.index]?.label}
				onpick={pick}
				onclose={close}
			/>
		{/key}
	{/if}
</header>

<style>
	.caption {
		display: flex;
		width: 46px;
		height: 100%;
		align-items: center;
		justify-content: center;
		border: none;
		background: transparent;
		color: var(--color-fg-secondary);
	}
	.caption:hover {
		background: var(--color-hover);
		color: var(--color-fg);
	}
	.caption.close:hover {
		background: var(--color-danger);
		color: var(--color-on-danger);
	}
	.caption svg {
		width: 10px;
		height: 10px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1;
	}
</style>
