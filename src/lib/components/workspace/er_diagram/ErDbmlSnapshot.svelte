<script lang="ts">
	import { Icon, IconButton, Pill, Textarea } from '$lib/components/core'

	interface Props {
		text: string
		busy: boolean
		copied: boolean
		oncopy: () => void
		onclose: () => void
	}

	let { text, busy, copied, oncopy, onclose }: Props = $props()
</script>

<aside
	class="flex w-[min(22rem,36vw)] shrink-0 flex-col border-l border-line bg-panel"
	aria-label="DBML text view"
>
	<header
		class="flex shrink-0 items-center gap-2 border-b border-line bg-elevated px-2.5 py-2"
	>
		<span
			class="grid size-6 shrink-0 place-items-center rounded-md bg-input"
			aria-hidden="true"
		>
			<Icon name="code" class="size-3.5 text-info" />
		</span>
		<div class="flex min-w-0 flex-1 items-center gap-1.5">
			<h2 class="m-0 min-w-0 truncate font-mono text-label font-semibold text-fg">
				DBML
			</h2>
			<Pill tone="neutral" uppercase>{busy ? 'Generating' : 'Snapshot'}</Pill>
		</div>
		<IconButton
			label={copied ? 'Copied DBML' : 'Copy DBML'}
			size="sm"
			variant="quiet"
			disabled={busy && !text}
			onclick={oncopy}
		>
			<Icon name={copied ? 'check' : 'copy'} />
		</IconButton>
		<IconButton
			label="Close DBML snapshot"
			size="sm"
			variant="quiet"
			onclick={onclose}
		>
			<Icon name="close" />
		</IconButton>
	</header>
	<div class="flex min-h-0 flex-1 flex-col bg-input">
		{#if busy && !text}
			<p class="p-3 text-caption text-fg-muted">Generating DBML…</p>
		{:else}
			<Textarea
				class="!min-h-0 flex-1 rounded-none border-0 bg-transparent font-mono text-caption leading-relaxed"
				readonly
				wrap="off"
				spellcheck={false}
				resize="none"
				aria-label="DBML snapshot"
				value={text}
			/>
		{/if}
	</div>
</aside>
