<script lang="ts">
	/**
	 * Non-modal dock for agent and MCP writes that wait for the user
	 * ("Ask me for high-risk writes"). Approve once runs that exact
	 * statement one time; Deny, or 90 seconds without an answer, refuses it.
	 */
	import { Button } from '#core/ui'
	import { onMount } from 'svelte'
	import { callerLabel, needLabel, secondsLeft } from '../api'
	import { approvalQueue } from '../queue.svelte'

	const pending = $derived(approvalQueue.pending)

	onMount(() => approvalQueue.mount())
</script>

{#if pending.length > 0}
	<section
		class="fixed top-14 right-5 z-50 flex max-h-[70vh] w-[26rem] max-w-[calc(100vw-2.5rem)] flex-col overflow-hidden rounded-lg border border-line bg-raised shadow-md"
		aria-label="Write approvals"
		aria-live="polite"
	>
		<header class="border-b border-line px-3.5 py-2.5">
			<p class="m-0 text-label font-semibold text-fg">
				{pending.length === 1
					? 'A write is waiting for you'
					: `${pending.length} writes are waiting for you`}
			</p>
			<p class="m-0 mt-0.5 text-caption text-fg-muted">
				Approve once runs exactly this statement, one time.
			</p>
		</header>
		<ul class="m-0 flex list-none flex-col divide-y divide-line overflow-y-auto p-0">
			{#each pending as req (req.id)}
				{@const left = secondsLeft(req.expiresAt, approvalQueue.now)}
				<li class="flex flex-col gap-2 px-3.5 py-3">
					<div class="flex items-baseline justify-between gap-2">
						<p class="m-0 min-w-0 truncate text-label font-medium text-fg">
							{callerLabel(req.caller)} · {req.connectionName}
							{#if req.database}
								<span class="font-normal text-fg-muted">/ {req.database}</span>
							{/if}
						</p>
						<span
							class="shrink-0 font-mono text-caption tabular-nums text-fg-muted"
							aria-label={`${left} seconds left`}>{left}s</span
						>
					</div>
					<p class="m-0 text-caption text-fg-muted">
						<span class={req.environment === 'production' ? 'text-danger' : ''}
							>{req.environment}</span
						>
						· {needLabel(req.needed, req.granted)}
					</p>
					<pre
						class="m-0 max-h-40 overflow-auto rounded-md border border-line bg-field p-2 font-mono text-caption whitespace-pre-wrap text-fg">{req.sql}</pre>
					<div class="flex justify-end gap-1.5">
						<Button
							size="sm"
							variant="ghost"
							disabled={approvalQueue.answering[req.id]}
							onclick={() => void approvalQueue.decide(req.id, false)}
						>
							Deny
						</Button>
						<Button
							size="sm"
							variant={req.environment === 'production' ? 'danger' : 'primary'}
							disabled={approvalQueue.answering[req.id] || left === 0}
							onclick={() => void approvalQueue.decide(req.id, true)}
						>
							Approve once
						</Button>
					</div>
				</li>
			{/each}
		</ul>
		{#if approvalQueue.error}
			<p class="m-0 border-t border-line px-3.5 py-2 text-caption text-danger" role="alert">
				{approvalQueue.error}
			</p>
		{/if}
	</section>
{/if}
