<script lang="ts">
	import { Kbd, Modal } from '$lib/components/core'
	import {
		APP_SHORTCUT_GROUPS,
		isMacPlatform,
		labelForKey,
	} from '$lib/data/appShortcuts'

	interface Props {
		open: boolean
		onclose: () => void
	}

	let { open, onclose }: Props = $props()

	const mac = isMacPlatform()
</script>

<Modal
	{open}
	{onclose}
	title="Keyboard shortcuts"
	labelledby="shortcuts-title"
	layout="chrome"
	size="md"
	height="fill"
>
	<div class="min-h-0 flex-1 overflow-auto px-4 py-3">
		{#each APP_SHORTCUT_GROUPS as group, gi (group.id)}
			<section class={gi > 0 ? 'mt-4' : ''}>
				<h3
					class="m-0 mb-1.5 text-caption font-medium tracking-wide text-fg-muted uppercase"
				>
					{group.title}
				</h3>
				<ul class="m-0 list-none p-0">
					{#each group.items as item (item.id)}
						<li
							class="flex items-center justify-between gap-3 border-b border-line py-1.5 last:border-b-0"
						>
							<span class="min-w-0 text-body text-fg">{item.label}</span>
							<span
								class="flex shrink-0 items-center gap-1 [&_kbd]:min-w-6 [&_kbd]:px-1.5 [&_kbd]:py-0.5 [&_kbd]:text-center [&_kbd]:text-label [&_kbd]:text-fg"
							>
								{#each item.keys as token, i (item.id + i)}
									<Kbd>{labelForKey(token, mac)}</Kbd>
								{/each}
								{#if item.orKeys}
									<span class="px-0.5 text-caption text-fg-muted">or</span>
									{#each item.orKeys as token, i (item.id + '-or-' + i)}
										<Kbd>{labelForKey(token, mac)}</Kbd>
									{/each}
								{/if}
							</span>
						</li>
					{/each}
				</ul>
			</section>
		{/each}
	</div>
</Modal>
