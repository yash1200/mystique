<script lang="ts">
	import { Button } from '#core/ui'
	import { fade } from 'svelte/transition'
	import type { LiveRun } from '../live-run'
	import AgentToolCard from './AgentToolCard.svelte'
	import ChatMarkdown from './ChatMarkdown.svelte'

	/** The reply as it streams: text and tool calls in the order they came. */
	interface Props {
		run: LiveRun
		engine: string
		workingLabel: string
		onstop: () => void
	}

	let { run, engine, workingLabel, onstop }: Props = $props()

	let showReasoning = $state(false)
	const last = $derived(run.parts.at(-1))
	/** Waiting on the model, not on a tool: show the typing row. */
	const thinking = $derived(
		!last || (last.kind === 'tool' && last.tool.status !== 'running' && last.tool.status !== 'waiting'),
	)
</script>

{#if run.reasoning}
	<li class="self-stretch">
		<button
			type="button"
			class="rounded-md px-1 py-0.5 text-caption text-fg-muted transition-colors duration-150 hover:bg-hover hover:text-fg-secondary"
			aria-expanded={showReasoning}
			onclick={() => (showReasoning = !showReasoning)}
		>
			{showReasoning ? 'Hide reasoning' : 'Show reasoning'}
		</button>
		{#if showReasoning}
			<p
				class="m-0 mt-1 max-h-40 overflow-auto rounded-md bg-elevated px-2 py-1.5 text-caption whitespace-pre-wrap text-fg-muted"
			>
				{run.reasoning.slice(-4000)}
			</p>
		{/if}
	</li>
{/if}
{#each run.parts as part, i (part.kind === 'tool' ? `tool-${part.tool.id}` : `text-${i}`)}
	{#if part.kind === 'tool'}
		<li class="self-stretch">
			<AgentToolCard
				name={part.tool.name}
				{engine}
				status={part.tool.status}
				summary={part.tool.summary}
				args={part.tool.args}
				approvalSql={part.tool.approvalSql}
			/>
		</li>
	{:else if part.text.trim()}
		<li class="self-stretch" data-chat-message aria-live="polite">
			<ChatMarkdown source={part.text} {engine} />
		</li>
	{/if}
{/each}
{#if run.compacted > 0}
	<li class="self-stretch text-caption text-fg-muted">
		Older messages were left out to fit the model's context.
	</li>
{/if}
<li
	class="flex items-center gap-2 self-stretch rounded-lg bg-elevated px-2.5 py-2"
	aria-live="polite"
	transition:fade={{ duration: 160 }}
>
	{#if thinking}
		<span class="agent-typing" aria-hidden="true">
			<span></span>
			<span></span>
			<span></span>
		</span>
	{/if}
	<p class="m-0 min-w-0 flex-1 text-caption text-fg-secondary">
		{run.stopRequested ? 'Stopping…' : workingLabel}
	</p>
	<Button size="sm" variant="ghost" onclick={onstop} disabled={run.stopRequested}>
		Stop
	</Button>
</li>

<style>
	.agent-typing {
		display: inline-flex;
		align-items: center;
		gap: 0.1875rem;
	}

	.agent-typing span {
		display: block;
		width: 0.25rem;
		height: 0.25rem;
		border-radius: 999px;
		background: var(--accent);
		opacity: 0.28;
		animation: agent-typing 1s var(--ease-in-out) infinite;
	}

	.agent-typing span:nth-child(2) {
		animation-delay: 0.14s;
	}

	.agent-typing span:nth-child(3) {
		animation-delay: 0.28s;
	}

	@keyframes agent-typing {
		0%,
		100% {
			opacity: 0.28;
			transform: translateY(0);
		}
		50% {
			opacity: 1;
			transform: translateY(-1px);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.agent-typing span {
			animation: none;
			opacity: 0.7;
		}
	}
</style>
