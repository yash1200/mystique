<script lang="ts">
	import {
		APP_SHORTCUT_GROUPS,
		isMacPlatform,
		labelForKey,
	} from '$lib/data/appShortcuts'
	import { modalPanel, overlayFade } from '$lib/utils/motion'
	import IconButton from './IconButton.svelte'
	import Kbd from './Kbd.svelte'

	interface Props {
		open: boolean
		onclose: () => void
	}

	let { open, onclose }: Props = $props()

	const mac = isMacPlatform()

	function onWindowKey(e: KeyboardEvent) {
		if (open && e.key === 'Escape') onclose()
	}
</script>

<svelte:window onkeydown={onWindowKey} />

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center p-4">
		<button
			type="button"
			class="absolute inset-0 border-none bg-black/45"
			aria-label="Dismiss"
			transition:overlayFade
			onclick={onclose}
		></button>
		<div
			class="relative z-10 flex h-[min(36rem,calc(100vh-2rem))] w-full max-w-lg flex-col overflow-hidden rounded-lg border border-line bg-elevated shadow-lg"
			transition:modalPanel
			role="dialog"
			aria-modal="true"
			aria-labelledby="shortcuts-title"
		>
			<header
				class="flex shrink-0 items-center justify-between gap-3 border-b border-line px-4 py-3"
			>
				<h2 id="shortcuts-title" class="m-0 text-body font-semibold text-fg">
					Keyboard shortcuts
				</h2>
				<IconButton
					label="Close"
					variant="ghost"
					size="sm"
					onclick={onclose}
				>
					<span class="text-body-lg leading-none" aria-hidden="true">×</span>
				</IconButton>
			</header>

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
		</div>
	</div>
{/if}
