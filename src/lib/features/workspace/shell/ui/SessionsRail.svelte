<script lang="ts">
	import { commands } from '#core/commands'
	import type { ConnectedSession } from '#core/contracts'
	import {
		ContextMenu,
		Draggable,
		Icon,
		IconButton,
		Mascot,
		type ContextMenuItem,
	} from '#core/ui'
	import { cn } from '#core/util'
	import { engineLabel, normalizeCapabilities } from '#engines'
	import {
		colorHex,
		connectionInitial,
		copyConnectionUrl as copyProfileUrl,
	} from '#features/connections'
	import { SettingsModal } from '#features/settings'
	import { appShortcutsUi } from '#features/shortcuts'
	import { cmd } from '#lib/app'
	import { onMount } from 'svelte'
	import { flip } from 'svelte/animate'
	import {
		hasPendingLoss,
		workspaceLoss,
		type PendingLoss,
		tabEditsAreDirty,
		resetAppToHome,
		workspaceSession,
		workspaceStudio,
	} from '#features/workspace/session'
	import { connectionInfo } from '../connection-info.svelte'
	import LeaveWorkspaceModal from './LeaveWorkspaceModal.svelte'

	export type SideNavMode = 'explorer' | 'saved-queries' | 'history'

	interface Props {
		onselect: (id: string) => void
		ondisconnect: (id: string) => void
		onrefresh: (id: string) => void
		onnewquery: (id: string) => void
		onmetrics?: (id: string) => void
		onopen: () => void
		onmode: (mode: SideNavMode) => void
		onreorder?: (movedId: string, beforeId: string | null) => void
	}

	let {
		onselect,
		ondisconnect,
		onrefresh,
		onnewquery,
		onmetrics,
		onopen,
		onmode,
		onreorder,
	}: Props = $props()

	const sessions = $derived(workspaceSession.sessions)
	const activeId = $derived(workspaceSession.activeConnectionId)
	const mode = $derived(workspaceStudio.sideMode)
	/** The dog pricks its ears while a production connection is in view. */
	const guarding = $derived(
		sessions.find((s) => s.profile.id === activeId)?.profile.environment === 'production',
	)

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
	let leaveLoss = $state<PendingLoss | null>(null)
	let leaveBusy = $state(false)

	/** Leave at once unless an edit or open transaction would be lost. */
	function requestLeaveHome() {
		if (leaveBusy) return
		const loss = workspaceLoss(
			workspaceSession.openTabs,
			workspaceSession.tabTxn,
			tabEditsAreDirty,
		)
		if (hasPendingLoss(loss)) leaveLoss = loss
		else void confirmLeaveHome()
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
		const s = sessionById(id)
		const caps = s
			? normalizeCapabilities(s.capabilities, String(s.profile.engine))
			: null
		onmode(caps && caps.supportsExplorer === false ? 'saved-queries' : 'explorer')
		onselect(id)
	}

	const menuSession = $derived.by(() => {
		const id = menu?.id
		if (!id) return null
		return sessions.find((s) => s.profile.id === id) ?? null
	})

	const supportsMetrics = $derived.by(() => {
		if (!menuSession) return false
		return normalizeCapabilities(
			menuSession.capabilities,
			String(menuSession.profile.engine),
		).supportsServerMetrics
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
		...(supportsMetrics ? [{ id: 'metrics', label: 'Server metrics' }] : []),
		{
			id: 'copy-url',
			label: 'Copy connection URL',
			separatorBefore: true,
		},
		{
			id: 'disconnect',
			label: 'Disconnect',
			danger: true,
			separatorBefore: true,
		},
	])

	function copyConnectionUrl(id: string) {
		const session = sessions.find((s) => s.profile.id === id)
		if (session) void copyProfileUrl(session.profile)
	}

	function onMenuPick(action: string) {
		const id = menu?.id
		menu = null
		if (!id) return
		if (action === 'info') connectionInfo.id = id
		if (action === 'refresh') onrefresh(id)
		if (action === 'new-query') onnewquery(id)
		if (action === 'metrics') onmetrics?.(id)
		if (action === 'copy-url') void copyConnectionUrl(id)
		if (action === 'disconnect') ondisconnect(id)
	}

	onMount(() =>
		commands.register([
			{ ...cmd.goHome, run: () => requestLeaveHome() },
			{
				...cmd.copyConnectionUrl,
				when: () => activeId != null,
				run: () => {
					if (activeId) void copyConnectionUrl(activeId)
				},
			},
		]),
	)
</script>

<aside
	class="flex h-full w-12 shrink-0 flex-col items-center border-r border-line bg-side"
	aria-label="Workspace navigation"
>
	<header class="flex h-topbar w-full shrink-0 items-center justify-center">
		<button
			type="button"
			class="grid size-control place-items-center rounded-md border-none bg-transparent p-0 transition-colors duration-150 ease-standard hover:bg-hover"
			title="Back to connections"
			aria-label="Back to connections"
			onclick={requestLeaveHome}
		>
			<Mascot mood={guarding ? 'alert' : 'idle'} playful />
		</button>
	</header>

	<div
		bind:this={listEl}
		class="flex w-full flex-1 flex-col items-center gap-2 overflow-x-hidden overflow-y-auto px-1.5 py-1"
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
						'flex w-full flex-col items-center gap-2',
						drag.draggingId && 'cursor-grabbing select-none',
					)}
					role="listbox"
					aria-label="Open connections"
				>
					{#each sessions as s (s.profile.id)}
						{@const active = mode === 'explorer' && activeId === s.profile.id}
						{@const initial = connectionInitial(s.profile.name, String(s.profile.engine))}
						{@const label = `${s.profile.name} · ${engineLabel(s.profile.engine)}`}
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
							style:--connection-color={colorHex(s.profile.colorId)}
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
							<!-- Flush with the rail edge (8px left of the tile), like the side pane footer's stripe. -->
							<span
								class={cn(
									'absolute top-1/2 -left-2 h-4 w-stripe -translate-y-1/2 rounded-r-full bg-conn transition-[opacity,transform] duration-180 ease-standard',
									active ? 'opacity-100' : 'scale-y-0 opacity-0',
								)}
								aria-hidden="true"
							></span>
							<button
								type="button"
								draggable="false"
								class={cn(
									'conn-tile size-control rounded-md text-body transition-[box-shadow,transform] duration-150 ease-standard active:scale-[0.92]',
									active && 'conn-ring',
									canReorder && (drag.draggingId ? 'cursor-grabbing' : 'cursor-grab'),
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
								<span class="select-none" aria-hidden="true">{initial}</span>
							</button>
						</div>
					{/each}

					<IconButton label="Open another connection" variant="quiet" onclick={onopen}>
						<Icon name="plus" />
					</IconButton>
				</div>
			{/snippet}

			{#snippet preview({ id })}
				{@const s = sessionById(id)}
				{#if s}
					<span
						class="conn-tile size-control rounded-md text-body shadow-md"
						style:--connection-color={colorHex(s.profile.colorId)}
					>
						{connectionInitial(s.profile.name, String(s.profile.engine))}
					</span>
				{/if}
			{/snippet}
		</Draggable>

		<span class="h-px w-5 shrink-0 bg-line" aria-hidden="true"></span>

		<div class="flex flex-col items-center gap-1" role="toolbar" aria-label="Queries">
			<IconButton
				label="Saved queries"
				variant="quiet"
				active={mode === 'saved-queries'}
				onclick={() => onmode('saved-queries')}
			>
				<Icon name="bookmark" />
			</IconButton>
			<IconButton
				label="Query history"
				variant="quiet"
				active={mode === 'history'}
				onclick={() => onmode('history')}
			>
				<Icon name="history" />
			</IconButton>
		</div>
	</div>

	<footer class="flex w-full shrink-0 flex-col items-center gap-1 py-2">
		<IconButton
			label="Keyboard shortcuts"
			variant="quiet"
			active={appShortcutsUi.open}
			onclick={() => appShortcutsUi.toggle()}
			aria-expanded={appShortcutsUi.open}
			aria-haspopup="dialog"
		>
			<Icon name="help" />
		</IconButton>
		<SettingsModal buttonSize="md" buttonVariant="quiet" />
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

<LeaveWorkspaceModal
	loss={leaveLoss}
	busy={leaveBusy}
	onconfirm={() => void confirmLeaveHome()}
	oncancel={() => {
		if (!leaveBusy) leaveLoss = null
	}}
/>
