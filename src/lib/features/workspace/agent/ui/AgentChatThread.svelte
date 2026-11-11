<script lang="ts">
	import { Button, Icon } from '#core/ui'
	import type { LiveRun } from '../live-run'
	import { inferFenceLang, isQueryLang, parseChatMarkdown } from '../markdown'
	import type { AgentMessage } from '../types'
	import AgentLiveReply from './AgentLiveReply.svelte'
	import AgentToolCard from './AgentToolCard.svelte'
	import ChatMarkdown from './ChatMarkdown.svelte'

	interface Props {
		messages: AgentMessage[]
		engine: string
		/** The reply streaming now, after the saved messages. */
		live: LiveRun | null
		workingLabel: string
		onapply: (sql: string) => void
		onstop: () => void
	}

	let { messages, engine, live, workingLabel, onapply, onstop }: Props =
		$props()

	function messageHasApplyableFence(content: string): boolean {
		return parseChatMarkdown(content).some(
			(block) =>
				block.type === 'code' &&
				isQueryLang(inferFenceLang(block.lang, block.value)),
		)
	}
</script>

<ul class="m-0 flex list-none flex-col gap-4 p-0">
	{#each messages as msg, i (`${msg.role}-${String(msg.id)}-${i}`)}
		{#if msg.role === 'tool'}
			<li class="self-stretch">
				<AgentToolCard
					name={msg.toolName}
					{engine}
					content={String(msg.content ?? '')}
				/>
			</li>
		{:else if msg.role === 'user'}
			<li class="flex justify-end">
				<div
					class="min-w-0 max-w-[88%] rounded-xl rounded-br-xs bg-accent-soft px-3 py-2"
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
						<Button size="sm" onclick={() => onapply(msg.draftBody ?? '')}>
							<Icon name="code" />Apply to editor
						</Button>
					</div>
				{/if}
			</li>
		{/if}
	{/each}
	{#if live}
		<AgentLiveReply run={live} {engine} {workingLabel} {onstop} />
	{/if}
</ul>
