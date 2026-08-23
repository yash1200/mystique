<script lang="ts">
	import {
		ContextMenu,
		Draggable,
		Icon,
		IconButton,
		type ContextMenuItem,
	} from '$lib/components/core'
	import {
		BrandMark,
		KeyboardShortcutsModal,
		SettingsModal,
	} from '$lib/components/home'
	import type { ConnectedSession } from '$lib/stores/workspace-session.svelte'
	import {
		colorHex,
		connectionInitial,
		engineLabel,
	} from '$lib/types/connection'
	import { cn } from '$lib/utils/cn'
	import { resetAppToHome } from '$lib/utils/resetApp'
	import { flip } from 'svelte/animate'
	import ConnectionInfoModal from './ConnectionInfoModal.svelte'
	import LeaveWorkspaceModal from './LeaveWorkspaceModal.svelte'

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
		onreorder?: (movedId: string, beforeId: string | null) => void
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
		onreorder,
	}: Props = $props()

	const flipMs =
		typeof window !== 'undefined' &&
		window.matchMedia('(prefers-reduced-motion: reduce)').matches
			? 0
			: 140

	const sessionIds = $derived(sessions.map((s) => s.profile.id))
	const canReorder = $derived(!!onreorder && sessions.length > 1)
	let listEl = $state<HTMLDivElement | undefined>(undefined)

	function sessionById(id: string): ConnectedSession | undefined {
		return sessions.find((s) => s.profile.id === id)
	}

	let menu = $state<{ id: string; x: number; y: number } | null>(null)
	let infoId = $state<string | null>(null)
	let leaveOpen = $state(false)
	let leaveBusy = $state(false)
	let shortcutsOpen = $state(false)

	function requestLeaveHome() {
		if (leaveBusy) return
		leaveOpen = true
	}

	async function confirmLeaveHome() {
		if (leaveBusy) return
		leaveBusy = true
		try {
			await resetAppToHome()
		} finally {
			leaveBusy = false
		}
	}

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
		{ id: 'info', label: 'Show info' },
		{ id: 'refresh', label: 'Refresh catalog' },
		{ id: 'new-query', label: 'New query' },
		{
			id: 'disconnect',
			label: 'Disconnect',
			danger: true,
			separatorBefore: true,
		},
	])

	const infoProfile = $derived(
		infoId
			? (sessions.find((s) => s.profile.id === infoId)?.profile ?? null)
			: null,
	)

	function onMenuPick(action: string) {
		const id = menu?.id
		menu = null
		if (!id) return
		if (action === 'info') infoId = id
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
	>
		<button
			type="button"
			class="grid size-9 place-items-center rounded-lg border-none bg-transparent p-0 transition-opacity hover:opacity-85 focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1"
			title="Back to connections"
			aria-label="Back to connections"
			onclick={requestLeaveHome}
		>
			<BrandMark size="sm" />
		</button>
	</header>

	<div
		bind:this={listEl}
		class="flex w-full flex-1 flex-col items-center gap-1 overflow-y-auto overflow-x-hidden px-1.5 py-2"
	>
		<Draggable
			ids={sessionIds}
			enabled={canReorder}
			axis="y"
			scrollRoot={listEl}
			onpickup={selectConnection}
			onreorder={(movedId, beforeId) => onreorder?.(movedId, beforeId)}
		>
			{#snippet children(drag)}
				<div
					class={cn(
						'flex w-full flex-col items-center gap-1.5',
						drag.draggingId && 'cursor-grabbing select-none',
					)}
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
						{@const dragging = drag.draggingId === s.profile.id}
						{@const edge = drag.dropEdge(s.profile.id)}
						<div
							data-drag-id={s.profile.id}
							animate:flip={{ duration: flipMs }}
							class={cn(
								'relative',
								canReorder && !drag.draggingId && 'cursor-grab',
								dragging && 'pointer-events-none cursor-grabbing opacity-35',
							)}
							role="presentation"
							style="--connection-color: {hex}"
							onpointerdown={(e) => drag.start(e, s.profile.id)}
						>
							{#if edge === 'before' || edge === 'after'}
								<span
									class={cn(
										'pointer-events-none absolute inset-x-0 z-10 h-0.5 rounded-full bg-accent',
										edge === 'before' ? '-top-1' : '-bottom-1',
									)}
									aria-hidden="true"
								></span>
							{/if}
							<button
								type="button"
								draggable="false"
								class={cn(
									'relative grid size-9 place-items-center overflow-hidden rounded-lg border',
									'text-title leading-none font-semibold transition-[border-color,box-shadow,background-color] duration-150',
									'focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1',
									canReorder &&
										(drag.draggingId ? 'cursor-grabbing' : 'cursor-grab'),
									active
										? 'conn-ring conn-tint border-(--connection-color)'
										: prod
											? 'border-warning/45 bg-elevated hover:border-warning/70 hover:bg-warning/10'
											: 'border-line bg-elevated hover:border-line-strong hover:bg-hover',
								)}
								title={canReorder
									? `${label} · Drag to reorder · Right-click for actions`
									: `${label} · Right-click for actions`}
								role="option"
								aria-selected={active}
								aria-label={label}
								onclick={() => {
									if (drag.ateClick()) return
									selectConnection(s.profile.id)
								}}
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
						<Icon name="plus" class="size-4" />
					</button>
				</div>
			{/snippet}

			{#snippet preview({ id })}
				{@const s = sessionById(id)}
				{#if s}
					{@const hex = colorHex(s.profile.colorId)}
					{@const prod = s.profile.environment === 'production'}
					{@const initial = connectionInitial(
						s.profile.name,
						String(s.profile.engine),
					)}
					<div
						class={cn(
							'relative grid size-9 place-items-center overflow-hidden rounded-lg border',
							'text-title leading-none font-semibold',
							prod
								? 'border-warning/45 bg-elevated'
								: 'border-line bg-elevated',
						)}
						style="--connection-color: {hex}"
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
					</div>
				{/if}
			{/snippet}
		</Draggable>

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
				<Icon name="bookmark" class="size-4" />
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
				<Icon name="history" class="size-4" />
			</button>
		</div>
	</div>

	<footer
		class="flex min-h-statusbar w-full shrink-0 flex-col items-center justify-center gap-1 border-t border-line bg-elevated py-1.5"
		title="{sessions.length} session{sessions.length === 1 ? '' : 's'}"
	>
		<IconButton
			label="Keyboard shortcuts"
			variant="quiet"
			size="sm"
			active={shortcutsOpen}
			onclick={() => (shortcutsOpen = !shortcutsOpen)}
			aria-expanded={shortcutsOpen}
			aria-haspopup="dialog"
		>
			<Icon name="help" />
		</IconButton>
		<SettingsModal buttonSize="sm" buttonVariant="quiet" />
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

<ConnectionInfoModal
	open={infoProfile != null}
	profile={infoProfile}
	onclose={() => (infoId = null)}
/>

<LeaveWorkspaceModal
	open={leaveOpen}
	busy={leaveBusy}
	onconfirm={() => void confirmLeaveHome()}
	oncancel={() => {
		if (!leaveBusy) leaveOpen = false
	}}
/>

<KeyboardShortcutsModal
	open={shortcutsOpen}
	onclose={() => (shortcutsOpen = false)}
/>
