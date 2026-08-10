<script lang="ts">
	import {
		AppSettingsPopover,
		BrandMark,
		ContextMenu,
		type ContextMenuItem,
	} from '$lib/components/ui'
	import type { ConnectedSession } from '$lib/stores/workspace-session.svelte'
	import {
		colorHex,
		connectionInitial,
		engineLabel,
	} from '$lib/types/connection'
	import { cn } from '$lib/utils/cn'

	export type SideNavMode = 'explorer' | 'saved-queries' | 'history'

	interface Props {
		sessions: ConnectedSession[]
		activeId: string | null
		mode?: SideNavMode
		onselect: (id: string) => void
		ondisconnect: (id: string) => void
		onrefresh: (id: string) => void
		onnewquery: (id: string) => void
		onopen: () => void
		onmode: (mode: SideNavMode) => void
	}

	let {
		sessions,
		activeId,
		mode = 'explorer',
		onselect,
		ondisconnect,
		onrefresh,
		onnewquery,
		onopen,
		onmode,
	}: Props = $props()

	let menu = $state<{ id: string; x: number; y: number } | null>(null)

	function selectConnection(id: string) {
		onmode('explorer')
		onselect(id)
	}

	const menuSession = $derived.by(() => {
		const id = menu?.id
		if (!id) return null
		return sessions.find((s) => s.profile.id === id) ?? null
	})

	const menuItems = $derived.by((): ContextMenuItem[] => [
		{
			id: 'label',
			label: menuSession?.profile.name ?? 'Connection',
			disabled: true,
		},
		{ id: 'refresh', label: 'Refresh catalog' },
		{ id: 'new-query', label: 'New query' },
		{
			id: 'disconnect',
			label: 'Disconnect',
			danger: true,
			separatorBefore: true,
		},
	])

	function onMenuPick(action: string) {
		const id = menu?.id
		menu = null
		if (!id) return
		if (action === 'refresh') onrefresh(id)
		if (action === 'new-query') onnewquery(id)
		if (action === 'disconnect') ondisconnect(id)
	}
</script>

<aside
	class="flex h-full w-12 shrink-0 flex-col items-center border-r border-line bg-panel"
	aria-label="Workspace navigation"
>
	<header
		class="flex h-topbar w-full shrink-0 items-center justify-center border-b border-line"
		title="MystiqueDB"
	>
		<BrandMark size="sm" />
	</header>

	<div
		class="flex w-full flex-1 flex-col items-center gap-1 overflow-y-auto overflow-x-hidden px-1.5 py-2"
	>
		<div
			class="flex w-full flex-col items-center gap-1.5"
			role="listbox"
			aria-label="Open connections"
		>
			{#each sessions as s (s.profile.id)}
				{@const active = mode === 'explorer' && activeId === s.profile.id}
				{@const hex = colorHex(s.profile.colorId)}
				{@const prod = s.profile.environment === 'production'}
				{@const initial = connectionInitial(
					s.profile.name,
					String(s.profile.engine),
				)}
				{@const label = `${s.profile.name} · ${engineLabel(s.profile.engine)}${prod ? ' · PROD' : ''}`}
				<div
					role="option"
					aria-selected={active}
					style="--connection-color: {hex}"
				>
					<button
						type="button"
						class={cn(
							'relative grid size-9 place-items-center overflow-hidden rounded-lg border',
							'text-title leading-none font-semibold transition-[border-color,box-shadow,background-color] duration-150',
							'focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1',
							active
								? 'conn-ring conn-tint border-(--connection-color)'
								: prod
									? 'border-warning/45 bg-elevated hover:border-warning/70 hover:bg-warning/10'
									: 'border-line bg-elevated hover:border-line-strong hover:bg-hover',
						)}
						title="{label} · Right-click for actions"
						aria-label={label}
						aria-haspopup="menu"
						onclick={() => selectConnection(s.profile.id)}
						oncontextmenu={(e) => {
							e.preventDefault()
							e.stopPropagation()
							menu = { id: s.profile.id, x: e.clientX, y: e.clientY }
						}}
					>
						<span
							class="absolute inset-x-0 top-0 h-0.5 bg-conn"
							aria-hidden="true"
						></span>
						<span
							class="select-none"
							style="color: var(--connection-color)"
							aria-hidden="true"
						>
							{initial}
						</span>
						{#if prod}
							<span
								class="absolute top-1 right-1 size-1.5 rounded-full bg-warning ring-1 ring-elevated"
								title="Production"
								aria-hidden="true"
							></span>
						{/if}
					</button>
				</div>
			{/each}

			<button
				type="button"
				class={cn(
					'grid size-9 place-items-center rounded-lg border border-dashed border-line',
					'text-fg-muted transition-colors',
					'hover:border-accent hover:bg-accent-muted hover:text-accent',
					'focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1',
				)}
				title="Open another connection"
				aria-label="Open another connection"
				onclick={onopen}
			>
				<svg class="size-4" viewBox="0 0 16 16" fill="none" aria-hidden="true">
					<path
						d="M8 3.25v9.5M3.25 8h9.5"
						stroke="currentColor"
						stroke-width="1.4"
						stroke-linecap="round"
					/>
				</svg>
			</button>
		</div>

		<div
			class="mt-1 flex w-full flex-col items-center gap-1.5 border-t border-line pt-2"
			role="toolbar"
			aria-label="Queries"
		>
			<button
				type="button"
				class={cn(
					'grid size-9 place-items-center rounded-lg border transition-colors',
					'focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1',
					mode === 'saved-queries'
						? 'border-accent bg-accent-muted text-accent'
						: 'border-line bg-elevated text-fg-muted hover:border-line-strong hover:bg-hover hover:text-fg',
				)}
				title="Saved queries"
				aria-label="Saved queries"
				aria-pressed={mode === 'saved-queries'}
				onclick={() => onmode('saved-queries')}
			>
				<svg class="size-4" viewBox="0 0 16 16" fill="none" aria-hidden="true">
					<path
						d="M4.25 2.75h7.5v10.5L8 11.1l-3.75 2.15V2.75z"
						stroke="currentColor"
						stroke-width="1.3"
						stroke-linejoin="round"
					/>
				</svg>
			</button>

			<button
				type="button"
				class={cn(
					'grid size-9 place-items-center rounded-lg border transition-colors',
					'focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1',
					mode === 'history'
						? 'border-accent bg-accent-muted text-accent'
						: 'border-line bg-elevated text-fg-muted hover:border-line-strong hover:bg-hover hover:text-fg',
				)}
				title="Query history"
				aria-label="Query history"
				aria-pressed={mode === 'history'}
				onclick={() => onmode('history')}
			>
				<svg class="size-4" viewBox="0 0 16 16" fill="none" aria-hidden="true">
					<circle
						cx="8"
						cy="8"
						r="5.25"
						stroke="currentColor"
						stroke-width="1.3"
					/>
					<path
						d="M8 5.25V8l2 1.5"
						stroke="currentColor"
						stroke-width="1.3"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				</svg>
			</button>
		</div>
	</div>

	<footer
		class="flex min-h-statusbar w-full shrink-0 flex-col items-center justify-center gap-1 border-t border-line bg-elevated py-1.5"
		title="{sessions.length} session{sessions.length === 1 ? '' : 's'}"
	>
		<AppSettingsPopover placement="right-end" buttonSize="sm" />
		<span class="font-mono text-caption tabular-nums text-fg-muted">
			{sessions.length}
		</span>
	</footer>
</aside>

{#if menu}
	<ContextMenu
		x={menu.x}
		y={menu.y}
		items={menuItems}
		ariaLabel="Connection actions"
		onpick={onMenuPick}
		onclose={() => (menu = null)}
	/>
{/if}
