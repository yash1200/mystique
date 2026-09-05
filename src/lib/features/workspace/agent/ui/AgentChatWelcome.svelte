<script lang="ts">
	import { Icon } from '$core/ui'
	import { cn } from '$core/util'
	import { labradorRest, labradorSit } from '$illustrations'

	const STARTERS = [
		{
			label: 'Map the schema',
			text: 'What are the main tables and how do they relate?',
			icon: 'schema' as const,
		},
		{
			label: 'Start from the editor',
			text: 'Explain the query in the editor and suggest a cleaner version.',
			icon: 'code' as const,
		},
		{
			label: 'Draft a first query',
			text: 'Draft a readable query to explore the most important table.',
			icon: 'table' as const,
		},
	] as const

	interface Props {
		sendChord: string
		loaf?: boolean
		onpick: (text: string) => void
	}

	let { sendChord, loaf = false, onpick }: Props = $props()
</script>

<div class="flex min-h-full flex-col">
	<div class="m-auto flex w-full max-w-panel-sm flex-col py-4">
		<span
			class="grid size-10 place-items-center rounded-xl bg-accent-muted text-accent"
			aria-hidden="true"
		>
			<Icon name="chat" class="size-5" />
		</span>
		<p
			class="mt-3 mb-0 text-pretty text-body-lg font-semibold tracking-tight text-fg"
		>
			Ask in the language of the schema
		</p>
		<p class="mt-1.5 mb-0 text-caption leading-relaxed text-fg-muted">
			Drafts land in the editor. You run them. The agent can read tables and
			probe with SELECT only.
		</p>
		<ul class="mt-4 mb-0 flex list-none flex-col gap-1 p-0">
			{#each STARTERS as starter (starter.label)}
				<li>
					<button
						type="button"
						class="flex w-full items-start gap-2 rounded-lg border border-line bg-elevated px-2.5 py-2 text-left transition-[border-color,background-color,transform] duration-150 ease-out hover:border-line-strong hover:bg-hover enabled:active:scale-[0.99]"
						onclick={() => onpick(starter.text)}
					>
						<Icon name={starter.icon} class="mt-0.5 text-accent" />
						<span class="min-w-0">
							<span class="block text-label font-medium text-fg">
								{starter.label}
							</span>
							<span class="mt-0.5 block text-caption text-fg-muted">
								{starter.text}
							</span>
						</span>
					</button>
				</li>
			{/each}
		</ul>
		<p class="mt-3 mb-0 text-caption text-fg-muted">{sendChord} to send</p>
	</div>
	<img
		src={loaf ? labradorRest : labradorSit}
		alt=""
		width={loaf ? 120 : 88}
		height={loaf ? 69 : 96}
		draggable="false"
		class={cn(
			'agent-labrador pointer-events-none mx-auto mt-auto mb-1 select-none',
			loaf ? 'w-28' : 'w-20',
		)}
	/>
</div>

<style>
	.agent-labrador {
		opacity: 0.22;
	}

	:global([data-theme='light']) .agent-labrador {
		opacity: 0.16;
	}
</style>
