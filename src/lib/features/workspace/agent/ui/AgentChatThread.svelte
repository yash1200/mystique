<script lang="ts">
	import { Button, Icon } from '#core/ui'
	import { cn } from '#core/util'
	import { fade } from 'svelte/transition'
	import { inferFenceLang, isQueryLang, parseChatMarkdown } from '../markdown'
	import type { AgentMessage } from '../types'
	import ChatMarkdown from './ChatMarkdown.svelte'

	interface Props {
		messages: AgentMessage[]
		engine: string
		sending: boolean
		workingLabel: string
		onapply: (sql: string) => void
		onstop: () => void
	}

	let { messages, engine, sending, workingLabel, onapply, onstop }: Props =
		$props()

	let openTools = $state<Record<string, boolean>>({})

	function messageHasApplyableFence(content: string): boolean {
		return parseChatMarkdown(content).some(
			(block) =>
				block.type === 'code' &&
				isQueryLang(inferFenceLang(block.lang, block.value)),
		)
	}

	function toolLabel(name?: string | null): string {
		switch (name) {
			case 'get_schema_dbml':
				return 'Fetched schema'
			case 'list_objects':
				return 'Listed objects'
			case 'describe_object':
				return 'Described object'
			case 'execute_readonly_query':
				return 'Probed rows'
			case 'list_saved_queries':
				return 'Listed saved queries'
			case 'list_connections':
				return 'Listed connections'
			case 'save_query':
				return 'Saved query'
			default:
				return name?.replace(/_/g, ' ') || 'Tool'
		}
	}

	function toggleTool(id: string) {
		openTools = { ...openTools, [id]: !openTools[id] }
	}

	function looksLikeJson(text: string): boolean {
		const t = text.trim()
		return t.startsWith('{') || t.startsWith('[')
	}

	function fenceAs(lang: string, text: string): string {
		return '```' + lang + '\n' + text.replace(/\s+$/, '') + '\n```'
	}
</script>

<ul class="m-0 flex list-none flex-col gap-4 p-0">
	{#each messages as msg, i (`${msg.role}-${String(msg.id)}-${i}`)}
		{#if msg.role === 'tool'}
			<li class="self-stretch">
				<button
					type="button"
					class="flex w-full items-center gap-1.5 rounded-md px-1 py-1 text-left transition-colors duration-150 ease-out hover:bg-hover"
					aria-expanded={openTools[msg.id] === true}
					onclick={() => toggleTool(msg.id)}
				>
					<Icon name="schema" class="text-fg-muted" />
					<span class="min-w-0 flex-1 truncate text-caption text-fg-secondary">
						{toolLabel(msg.toolName)}
					</span>
					<Icon
						name="chevron-down"
						class={cn(
							'text-fg-muted transition-transform duration-150 ease-out',
							openTools[msg.id] && 'rotate-180',
						)}
					/>
				</button>
				{#if openTools[msg.id]}
					<div
						class="mt-1 overflow-hidden rounded-lg border border-line bg-elevated p-2"
						data-chat-message
						transition:fade={{ duration: 140 }}
					>
						<ChatMarkdown
							source={looksLikeJson(String(msg.content ?? ''))
								? fenceAs('json', String(msg.content ?? ''))
								: String(msg.content ?? '')}
							{engine}
							compact
						/>
					</div>
				{/if}
			</li>
		{:else if msg.role === 'user'}
			<li class="flex justify-end">
				<div
					class="min-w-0 max-w-[88%] rounded-xl rounded-br-sm bg-accent-muted px-3 py-2"
					data-chat-message
				>
					<ChatMarkdown source={String(msg.content ?? '')} {engine} compact />
				</div>
			</li>
		{:else}
			<li class="self-stretch" data-chat-message>
				<ChatMarkdown
					source={String(msg.content ?? '')}
					{engine}
					onApply={onapply}
				/>
				{#if msg.draftBody && !messageHasApplyableFence(msg.content)}
					<div class="mt-2">
						<Button
							size="sm"
							variant="secondary"
							onclick={() => onapply(msg.draftBody ?? '')}
						>
							Apply to editor
						</Button>
					</div>
				{/if}
			</li>
		{/if}
	{/each}
	{#if sending}
		<li
			class="flex items-center gap-2 self-stretch rounded-lg bg-elevated px-2.5 py-2"
			aria-live="polite"
			transition:fade={{ duration: 160 }}
		>
			<span class="agent-typing" aria-hidden="true">
				<span></span>
				<span></span>
				<span></span>
			</span>
			<p class="m-0 min-w-0 flex-1 text-caption text-fg-secondary">
				{workingLabel}
			</p>
			<Button size="sm" variant="ghost" onclick={onstop}>Stop</Button>
		</li>
	{/if}
</ul>

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
