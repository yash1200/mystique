<script lang="ts">
	import { Icon } from '#core/ui'
	import { cn } from '#core/util'
	import { fade } from 'svelte/transition'
	import type { LiveToolStatus } from '../live-run'
	import { toolLabel, toolTarget } from '../tool-label'
	import ChatMarkdown from './ChatMarkdown.svelte'

	/**
	 * One tool call. Generic by tool name: a live call shows its status and
	 * summary; a saved one opens to its full body (args and result).
	 */
	interface Props {
		name: string | null | undefined
		engine: string
		status?: LiveToolStatus
		summary?: string
		args?: unknown
		/** A statement waiting in the approval dock. */
		approvalSql?: string
		/** Saved card body (JSON). Absent while the call is live. */
		content?: string | null
	}

	let {
		name,
		engine,
		status = 'ok',
		summary = '',
		args = undefined,
		approvalSql = undefined,
		content = null,
	}: Props = $props()

	let open = $state(false)

	const running = $derived(status === 'running' || status === 'waiting')
	const target = $derived(toolTarget(args ?? parsedArgs(content)))
	const body = $derived(content ? fenced(content) : '')

	function parsedArgs(raw: string | null): unknown {
		if (!raw) return undefined
		try {
			return (JSON.parse(raw) as { args?: unknown }).args
		} catch {
			return undefined
		}
	}

	function fenced(text: string): string {
		const t = text.trim()
		const json = t.startsWith('{') || t.startsWith('[')
		return json ? '```json\n' + t + '\n```' : text
	}
</script>

<div class="self-stretch">
	<button
		type="button"
		class="flex w-full items-center gap-1.5 rounded-md px-1 py-1 text-left transition-colors duration-150 ease-out enabled:hover:bg-hover"
		aria-expanded={body ? open : undefined}
		disabled={!body}
		onclick={() => (open = !open)}
	>
		{#if running}
			<span class="agent-tool-spinner shrink-0" aria-hidden="true"></span>
		{:else if status === 'error'}
			<Icon name="alert-circle" class="shrink-0 text-danger" />
		{:else}
			<Icon name="schema" class="shrink-0 text-fg-muted" />
		{/if}
		<span class="min-w-0 flex-1 truncate text-caption text-fg-secondary">
			{toolLabel(name, running)}
			{#if target}
				<span class="font-mono text-fg-muted">· {target}</span>
			{/if}
		</span>
		{#if summary && !running}
			<span
				class={cn(
					'max-w-[45%] shrink-0 truncate text-caption',
					status === 'error' ? 'text-danger' : 'text-fg-muted',
				)}
				title={summary}
			>
				{summary}
			</span>
		{/if}
		{#if body}
			<Icon
				name="chevron-down"
				class={cn(
					'shrink-0 text-fg-muted transition-transform duration-150 ease-out',
					open && 'rotate-180',
				)}
			/>
		{/if}
	</button>
	{#if status === 'waiting'}
		<p class="m-0 mt-1 rounded-md bg-warning/10 px-2 py-1.5 text-caption text-fg-secondary">
			Waiting for your approval in the dock{approvalSql ? ':' : '.'}
			{#if approvalSql}
				<code class="font-mono text-fg">{approvalSql.slice(0, 160)}</code>
			{/if}
		</p>
	{/if}
	{#if open && body}
		<div
			class="mt-1 overflow-hidden rounded-lg border border-line bg-raised p-2"
			data-chat-message
			transition:fade={{ duration: 140 }}
		>
			<ChatMarkdown source={body} {engine} compact />
		</div>
	{/if}
</div>

<style>
	.agent-tool-spinner {
		width: 0.75rem;
		height: 0.75rem;
		border-radius: 999px;
		border: 1.5px solid var(--line-2);
		border-top-color: var(--accent);
		animation: agent-tool-spin 0.8s linear infinite;
	}

	@keyframes agent-tool-spin {
		to {
			transform: rotate(360deg);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.agent-tool-spinner {
			animation: none;
		}
	}
</style>
