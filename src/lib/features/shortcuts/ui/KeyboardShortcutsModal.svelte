<script lang="ts">
	import { commands } from '#core/commands'
	import {
		Icon,
		IconButton,
		Input,
		Kbd,
		Mascot,
		Modal,
		Segmented,
		type IconName,
	} from '#core/ui'
	import { cn } from '#core/util'
	import { showToast } from '#features/toast'
	import { cmd } from '#lib/app'
	import { onMount } from 'svelte'
	import { appShortcutsUi } from '../chrome.svelte'
	import {
		isMacPlatform,
		isSecretQuery,
		keyCap,
		shortcutGroups,
		shortcutItem,
		shortcutMatches,
		type KeyLabelStyle,
		type ShortcutGroup,
		type ShortcutItem,
	} from '../data'

	interface Props {
		open: boolean
		onclose: () => void
	}

	let { open, onclose }: Props = $props()

	const TITLE_ID = 'shortcuts-title'
	const FILTER_ID = 'shortcuts-filter'
	const PET_MS = 1800

	const GROUP_ICONS: Record<string, IconName> = {
		app: 'settings',
		home: 'database',
		workspace: 'sidebar',
		query: 'code',
		grid: 'table',
		model: 'model',
	}

	const LABEL_STYLES = [
		{ value: 'glyphs', label: '⌘ Glyphs' },
		{ value: 'words', label: 'Words' },
	]

	const mac = isMacPlatform()
	// The keymap is declared at startup and does not change after.
	const groups = shortcutGroups(mac)
	const petKeys = shortcutItem(cmd.petMystique, mac)?.keys ?? []

	let activeId = $state(groups[0]?.id ?? 'app')
	let query = $state('')
	let petting = $state(false)
	let petTimer: ReturnType<typeof setTimeout> | undefined

	const q = $derived(query.trim())
	const current = $derived(groups.find((g) => g.id === activeId) ?? groups[0]!)
	/** Filtering searches every group; otherwise the picked group alone. */
	const sections = $derived<ShortcutGroup[]>(
		q
			? groups
					.map((g) => ({ ...g, items: g.items.filter((item) => shortcutMatches(item, q, mac)) }))
					.filter((g) => g.items.length > 0)
			: [current],
	)
	const matches = $derived(sections.reduce((n, g) => n + g.items.length, 0))
	const secret = $derived(isSecretQuery(q))

	function cap(token: string): string {
		return keyCap(token, appShortcutsUi.labels, mac)
	}

	/** The name split around the first match of the filter. */
	function nameParts(label: string) {
		const i = q ? label.toLowerCase().indexOf(q.toLowerCase()) : -1
		if (i < 0) return { before: label, hit: '', after: '' }
		return {
			before: label.slice(0, i),
			hit: label.slice(i, i + q.length),
			after: label.slice(i + q.length),
		}
	}

	function pet() {
		appShortcutsUi.pet()
		petting = true
		clearTimeout(petTimer)
		petTimer = setTimeout(() => (petting = false), PET_MS)
	}

	function close() {
		query = ''
		onclose()
	}

	onMount(() => {
		const unregister = commands.register([
			{
				...cmd.petMystique,
				run: () => {
					pet()
					if (!appShortcutsUi.open) showToast('success', appShortcutsUi.petLine)
				},
			},
		])
		return () => {
			unregister()
			clearTimeout(petTimer)
		}
	})
</script>

{#snippet keys(tokens: string[])}
	{#each tokens as token, i (i)}
		<Kbd>{cap(token)}</Kbd>
	{/each}
{/snippet}

{#snippet row(item: ShortcutItem)}
	{@const name = nameParts(item.label)}
	<li class="sc-row flex min-h-[2.875rem] items-center gap-3 px-4 transition-colors duration-150 hover:bg-hover">
		<span class="min-w-0 flex-1 truncate">
			{name.before}{#if name.hit}<mark class="rounded-xs bg-accent-soft text-accent-text"
					>{name.hit}</mark
				>{/if}{name.after}
		</span>
		<span class="flex shrink-0 items-center gap-1">
			{@render keys(item.keys)}
			{#if item.orKeys}
				<span class="px-1 text-label text-fg-muted">or</span>
				{@render keys(item.orKeys)}
			{/if}
		</span>
	</li>
{/snippet}

<Modal {open} onclose={close} layout="bare" size="lg" height="fill" labelledby={TITLE_ID}>
	<div class="flex h-full min-h-0 w-full flex-wrap">
		<nav
			class="flex max-w-full flex-[1_1_13.125rem] flex-col gap-0.5 border-r border-line bg-surface px-2.5 pt-[1.125rem] pb-3.5"
			aria-label="Shortcut categories"
		>
			<h2 id={TITLE_ID} class="m-0 px-2.5 pb-3 text-heading font-semibold">Shortcuts</h2>
			{#each groups as group (group.id)}
				{@const on = !q && activeId === group.id}
				<button
					type="button"
					class={cn(
						'group flex h-[2.125rem] w-full items-center gap-2.5 rounded-md border-none px-2.5 text-left font-medium transition-colors duration-150',
						on
							? 'bg-hover text-fg shadow-[inset_0_0_0_1px_var(--line)]'
							: 'bg-transparent text-fg-secondary hover:bg-hover hover:text-fg',
					)}
					aria-current={on ? 'page' : undefined}
					onclick={() => {
						activeId = group.id
						query = ''
					}}
				>
					<span class={cn('flex [&_svg]:size-4', on ? 'text-accent-text' : 'text-fg-muted')}>
						<Icon name={GROUP_ICONS[group.id] ?? 'help'} />
					</span>
					<span class="min-w-0 flex-1 truncate">{group.title}</span>
					<span class="font-mono text-caption text-fg-muted">{group.items.length}</span>
				</button>
			{/each}
		</nav>

		<section class="flex h-full min-w-0 flex-[999_1_26.25rem] flex-col">
			<header class="flex shrink-0 flex-col gap-3.5 border-b border-line px-6 pt-5 pb-4">
				<div class="flex items-center gap-3">
					<div class="flex min-w-0 flex-1 flex-col">
						<h3 class="m-0 truncate text-title leading-tight font-semibold tracking-[-0.015em]">
							{q ? 'Results' : current.title}
						</h3>
						<span class="text-fg-muted" aria-live="polite">
							{q
								? `${matches} matching ${matches === 1 ? 'shortcut' : 'shortcuts'}`
								: `${current.items.length} ${current.items.length === 1 ? 'shortcut' : 'shortcuts'}`}
						</span>
					</div>
					<Segmented
						size="sm"
						label="Key labels"
						options={LABEL_STYLES}
						value={appShortcutsUi.labels}
						onchange={(v) => (appShortcutsUi.labels = v as KeyLabelStyle)}
					/>
					<IconButton label="Close shortcuts" variant="quiet" size="sm" onclick={close}>
						<Icon name="close" />
					</IconButton>
				</div>
				<div class="relative">
					<label class="sr-only" for={FILTER_ID}>Filter shortcuts</label>
					<span class="pointer-events-none absolute top-1/2 left-3.5 flex -translate-y-1/2 text-fg-muted [&_svg]:size-4">
						<Icon name="search" />
					</span>
					<Input
						id={FILTER_ID}
						type="search"
						inputSize="lg"
						inset={query ? 'both' : 'start'}
						placeholder="Filter shortcuts…"
						bind:value={query}
						class="[&::-webkit-search-cancel-button]:hidden"
						onkeydown={(e) => {
							if (e.key !== 'Escape' || !query) return
							e.stopPropagation()
							query = ''
						}}
					/>
					{#if query}
						<button
							type="button"
							class="absolute top-1/2 right-2 grid size-6 -translate-y-1/2 place-items-center rounded-sm border-none bg-transparent text-fg-muted transition-colors hover:bg-hover hover:text-fg"
							aria-label="Clear filter"
							onclick={() => (query = '')}
						>
							<Icon name="close" class="size-3" />
						</button>
					{/if}
				</div>
			</header>

			<div class="flex min-h-0 flex-1 flex-col gap-5 overflow-auto px-6 pt-5 pb-7">
				{#each sections as section (section.id)}
					<div class="flex animate-rise flex-col gap-2.5">
						{#if q}
							<h4 class="m-0 label-caps">{section.title}</h4>
						{/if}
						<ul
							class="m-0 list-none overflow-hidden rounded-lg border border-line bg-surface p-0 [&>li+li]:border-t [&>li+li]:border-line"
							aria-label={section.title}
						>
							{#each section.items as item (item.id)}
								{@render row(item)}
							{/each}
						</ul>
					</div>
				{/each}

				{#if secret}
					<div class="flex animate-rise flex-col gap-2.5">
						<h4 class="m-0 label-caps">Found something</h4>
						<button
							type="button"
							class="sc-row flex min-h-[4.5rem] w-full items-center gap-3.5 rounded-lg border border-line bg-surface px-4 text-left transition-colors duration-150 hover:bg-hover"
							onclick={pet}
						>
							<span class="relative flex">
								<Mascot mood={petting ? 'happy' : 'idle'} wag={petting} size={44} />
								{#if petting}
									{#key appShortcutsUi.pets}
										<span class="heart absolute -top-1 -right-1.5 flex text-(--mascot-tongue) [&_svg]:size-4">
											<Icon name="heart" />
										</span>
									{/key}
								{/if}
							</span>
							<span class="flex min-w-0 flex-1 flex-col gap-0.5">
								<span class="font-medium">Pet Mystique</span>
								<span class="text-label text-fg-muted" aria-live="polite">{appShortcutsUi.petLine}</span>
							</span>
							<span class="flex shrink-0 items-center gap-1">{@render keys(petKeys)}</span>
						</button>
					</div>
				{:else if q && matches === 0}
					<div class="flex animate-rise flex-col items-center gap-1.5 px-4 py-10 text-center">
						<span class="font-medium">No shortcut for “{q}”</span>
						<span class="text-label text-fg-muted">
							Try “scale”, “settings” or a key like {cap('Mod')}.
						</span>
					</div>
				{/if}
			</div>
		</section>
	</div>
</Modal>

<style>
	/* Bigger caps in the list, and they press down while the row is hovered. */
	.sc-row :global(kbd) {
		min-width: 1.5rem;
		height: 1.5rem;
		padding: 0 0.4375rem;
		border-radius: 0.375rem;
		font-size: 0.71875rem;
		transition:
			transform var(--duration-fast) var(--ease-standard),
			border-bottom-width var(--duration-fast),
			color 0.15s;
	}
	.sc-row:hover :global(kbd) {
		transform: translateY(1px);
		border-bottom-width: 1px;
		color: var(--text);
	}
	.sc-row:hover :global(kbd:nth-child(2)) {
		transition-delay: 50ms;
	}
	.sc-row:hover :global(kbd:nth-child(3)) {
		transition-delay: 100ms;
	}
	.heart {
		animation: heart 0.9s var(--ease-standard) both;
	}
	@keyframes heart {
		0% {
			opacity: 0;
			transform: translateY(4px) scale(0.6);
		}
		40% {
			opacity: 1;
			transform: translateY(-4px) scale(1.1);
		}
		100% {
			opacity: 0;
			transform: translateY(-18px) scale(1);
		}
	}
</style>
