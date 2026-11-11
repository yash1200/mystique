<script lang="ts">
	import { Icon, Mascot } from '#core/ui'

	const STARTERS = [
		{ text: 'What are the main tables and how do they relate?', icon: 'schema' },
		{ text: 'Explain the query in the editor and suggest a cleaner version.', icon: 'code' },
		{ text: 'Draft a readable query to explore the most important table.', icon: 'table' },
	] as const

	interface Props {
		sendChord: string
		/** Late at night the dog naps. */
		sleepy?: boolean
		/** The connection (and database) the agent reads. */
		where?: string
		onpick: (text: string) => void
	}

	let { sendChord, sleepy = false, where, onpick }: Props = $props()
</script>

<div class="m-auto flex w-full max-w-panel-sm flex-col items-center gap-4 py-6 text-center">
	<Mascot mood={sleepy ? 'sleep' : 'idle'} size={44} playful />
	<div class="flex flex-col gap-1">
		<p class="m-0 text-body-lg font-semibold tracking-tight text-pretty">Ask about your data</p>
		<p class="m-0 text-label leading-relaxed text-pretty text-fg-muted">
			Drafts land in the editor and you run them. The agent reads
			{#if where}<span class="font-mono text-fg-secondary">{where}</span>{:else}the schema{/if}
			and probes with SELECT only.
		</p>
	</div>
	<ul class="m-0 flex w-full list-none flex-col gap-1.5 p-0 text-left">
		{#each STARTERS as starter (starter.text)}
			<li>
				<button
					type="button"
					class="flex w-full items-start gap-2.5 rounded-[10px] border-none bg-hover px-3 py-2.5 text-left text-label text-fg-secondary transition-[background-color,color,transform] duration-150 hover:bg-pressed hover:text-fg enabled:active:scale-[0.99]"
					onclick={() => onpick(starter.text)}
				>
					<span class="mt-px flex text-accent-text [&_svg]:size-3.5"><Icon name={starter.icon} /></span>
					{starter.text}
				</button>
			</li>
		{/each}
	</ul>
	<p class="m-0 text-caption text-fg-muted"><kbd>{sendChord}</kbd> sends</p>
</div>
