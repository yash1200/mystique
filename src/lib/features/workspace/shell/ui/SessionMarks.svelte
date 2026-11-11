<!--
	Mark at the end of the tab bar for the active connection: on a read-only
	connection, the lock, which offers a timed write unlock and shows its
	countdown while it runs.
-->
<script lang="ts">
	import type { ConnectionProfile } from '#core/contracts'
	import { ContextMenu, Icon, type ContextMenuItem } from '#core/ui'
	import { cn } from '#core/util'
	import {
		formatCountdown,
		TIMED_UNLOCK_CHOICES,
		timedUnlocks,
	} from '#features/workspace/session'

	interface Props {
		profile: ConnectionProfile
		engineSessionId: string | null
	}

	let { profile, engineSessionId }: Props = $props()

	const remaining = $derived(timedUnlocks.remainingMs(engineSessionId))

	let menu = $state<{ x: number; y: number } | null>(null)

	const items = $derived<ContextMenuItem[]>(
		remaining != null
			? [{ id: 'end', label: 'End now: back to read-only', icon: 'lock' }]
			: TIMED_UNLOCK_CHOICES.map((choice) => ({
					id: String(choice.seconds),
					label: choice.label,
					icon: 'lock-open' as const,
				})),
	)

	function openMenu(e: MouseEvent) {
		const box = (e.currentTarget as HTMLElement).getBoundingClientRect()
		menu = { x: box.left, y: box.bottom + 4 }
	}

	function pick(id: string) {
		menu = null
		const sid = engineSessionId
		if (!sid) return
		if (id === 'end') void timedUnlocks.end(sid)
		else void timedUnlocks.start(sid, Number(id))
	}
</script>

{#if profile.readOnly}
	<div
		class="flex h-control-sm shrink-0 items-center gap-1 border-b border-line bg-surface px-2"
		role="group"
		aria-label="Connection safety"
	>
		{#if profile.readOnly}
			<button
				type="button"
				class={cn(
					'inline-flex h-5 items-center gap-1 rounded-sm border-none bg-transparent px-1 text-caption leading-none transition-colors hover:bg-hover',
					remaining != null ? 'font-mono text-warning' : 'text-fg-muted hover:text-fg',
				)}
				title={remaining != null
					? 'Writes are allowed on this read-only connection until the timer ends. Click to end now.'
					: 'Read-only connection. Click to allow writes for a few minutes.'}
				aria-haspopup="menu"
				disabled={!engineSessionId}
				onclick={openMenu}
			>
				<Icon name={remaining != null ? 'lock-open' : 'lock'} class="size-3.5" />
				{#if remaining != null}
					<span aria-live="off">{formatCountdown(remaining)}</span>
					<span class="sr-only">left with writes allowed</span>
				{:else}
					<span>Read-only</span>
				{/if}
			</button>
		{/if}
	</div>
{/if}

{#if menu}
	<ContextMenu
		x={menu.x}
		y={menu.y}
		{items}
		placement="cursor"
		ariaLabel="Read-only connection"
		onpick={pick}
		onclose={() => (menu = null)}
	/>
{/if}
