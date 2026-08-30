<script lang="ts">
	import { Badge, Button, Pill } from '$core/ui'
	import type { McpHarness } from '../mcp-types'

	interface Props {
		harnesses: McpHarness[]
		busy: boolean
		busyId: string | null
		oninstall: (id: string) => void
	}

	let { harnesses, busy, busyId, oninstall }: Props = $props()

	function actionLabel(h: McpHarness): string {
		if (busyId === h.id) return h.stale ? 'Updating' : 'Adding'
		if (h.stale) return 'Update'
		return 'Add'
	}
</script>

<ul class="m-0 flex list-none flex-col divide-y divide-line p-0">
	{#each harnesses as h (h.id)}
		<li class="flex items-center gap-3 px-3 py-2.5">
			<span title={h.label} aria-hidden="true">
				<Badge>{h.mark}</Badge>
			</span>
			<div class="flex min-w-0 flex-1 items-baseline gap-1.5">
				<p class="m-0 truncate text-label font-medium text-fg">{h.label}</p>
				<span class="shrink-0 text-caption text-fg-muted">{h.hint}</span>
			</div>
			{#if h.installed && !h.stale}
				<Pill tone="success">Added</Pill>
			{:else}
				<Button size="sm" disabled={busy} onclick={() => oninstall(h.id)}>
					{actionLabel(h)}
				</Button>
			{/if}
		</li>
	{/each}
</ul>
