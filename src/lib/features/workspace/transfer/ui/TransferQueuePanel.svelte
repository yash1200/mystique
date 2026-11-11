<script lang="ts">
	/**
	 * Background exports and imports: one line per transfer with its row
	 * count, cancel while it runs, dismiss once it ends.
	 */
	import { Button, Icon, IconButton } from '#core/ui'
	import { cn } from '#core/util'
	import type { TransferQueue, TransferTask } from '../queue.svelte'

	interface Props {
		queue: TransferQueue
	}

	let { queue }: Props = $props()

	const tasks = $derived(queue.tasks)
	const finished = $derived(tasks.filter((t) => t.status !== 'queued' && t.status !== 'running'))

	function statusLine(task: TransferTask): string {
		const rows = `${task.rows.toLocaleString()} row${task.rows === 1 ? '' : 's'}`
		switch (task.status) {
			case 'queued':
				return 'Waiting for the previous transfer on this connection'
			case 'running':
				return task.kind === 'export' ? `Exporting… ${rows}` : `Importing… ${rows}`
			case 'done':
				return task.kind === 'export' ? `Exported ${rows}` : `Imported ${rows}`
			case 'cancelled':
				return task.message ?? 'Cancelled'
			case 'failed':
				return task.message ?? 'Failed'
		}
	}

	const tone: Record<TransferTask['status'], string> = {
		queued: 'text-fg-muted',
		running: 'text-fg-secondary',
		done: 'text-accent-text',
		cancelled: 'text-fg-muted',
		failed: 'text-danger',
	}
</script>

{#if tasks.length > 0}
	<section
		class="fixed bottom-[calc(var(--spacing-control)+theme(spacing.5))] left-5 z-40 flex w-panel-sm max-w-[calc(100vw-2.5rem)] flex-col gap-2 rounded-lg border border-line bg-raised px-3 py-2.5 text-caption text-fg shadow-md"
		aria-label="Background transfers"
	>
		<header class="flex items-center justify-between gap-2">
			<span class="font-medium text-fg-secondary">Transfers</span>
			{#if finished.length > 0}
				<Button variant="ghost" size="sm" onclick={() => queue.clearFinished()}>Clear</Button>
			{/if}
		</header>
		<ul class="m-0 flex list-none flex-col gap-1.5 p-0">
			{#each tasks as task (task.id)}
				<li class="flex items-start gap-2">
					<div class="min-w-0 flex-1">
						<p class="m-0 truncate" title={task.label}>
							{task.label}
							{#if task.throughTransaction}
								<span class="text-fg-muted">(with uncommitted changes)</span>
							{/if}
						</p>
						<p
							class={cn('m-0 truncate tabular-nums', tone[task.status])}
							role={task.status === 'failed' ? 'alert' : undefined}
							title={task.message ?? undefined}
						>
							{statusLine(task)}
						</p>
					</div>
					{#if task.status === 'queued' || task.status === 'running'}
						<Button variant="ghost" size="sm" onclick={() => void queue.cancel(task.id)}>
							Cancel
						</Button>
					{:else}
						<IconButton
							label="Dismiss"
							variant="ghost"
							size="sm"
							onclick={() => queue.dismiss(task.id)}
						>
							<Icon name="close" />
						</IconButton>
					{/if}
				</li>
			{/each}
		</ul>
	</section>
{/if}
