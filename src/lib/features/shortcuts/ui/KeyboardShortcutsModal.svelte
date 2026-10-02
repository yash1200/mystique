<script lang="ts">
	import { Icon, IconButton, Input, Kbd, Modal, type IconName } from '#core/ui'
	import { cn } from '#core/util'
	import {
		APP_SHORTCUT_GROUPS,
		isMacPlatform,
		labelForKey,
		type ShortcutItem,
	} from '../data'
	interface Props {
		open: boolean
		onclose: () => void
	}

	let { open, onclose }: Props = $props()

	const mac = isMacPlatform()
	let activeId = $state(APP_SHORTCUT_GROUPS[0]?.id ?? 'app')
	let query = $state('')

	const GROUP_ICONS: Record<string, IconName> = {
		app: 'settings',
		home: 'database',
		workspace: 'grid',
		query: 'code',
		grid: 'table',
		model: 'model',
	}

	const current = $derived(
		APP_SHORTCUT_GROUPS.find((g) => g.id === activeId) ??
			APP_SHORTCUT_GROUPS[0]!,
	)

	const filtering = $derived(query.trim().length > 0)

	const visible = $derived.by((): { group: string; item: ShortcutItem }[] => {
		const q = query.trim().toLowerCase()
		if (!q) {
			return current.items.map((item) => ({ group: current.title, item }))
		}
		return APP_SHORTCUT_GROUPS.flatMap((g) =>
			g.items
				.filter(
					(item) =>
						item.label.toLowerCase().includes(q) ||
						item.keys.some((k) =>
							labelForKey(k, mac).toLowerCase().includes(q),
						),
				)
				.map((item) => ({ group: g.title, item })),
		)
	})

	function matchParts(
		text: string,
		q: string,
	): { text: string; hit: boolean }[] {
		const needle = q.trim()
		if (!needle) return [{ text, hit: false }]
		const i = text.toLowerCase().indexOf(needle.toLowerCase())
		if (i < 0) return [{ text, hit: false }]
		return [
			{ text: text.slice(0, i), hit: false },
			{ text: text.slice(i, i + needle.length), hit: true },
			{ text: text.slice(i + needle.length), hit: false },
		].filter((part) => part.text.length > 0)
	}

	function close() {
		query = ''
		onclose()
	}
</script>

{#snippet chords(item: ShortcutItem)}
	<span class="flex shrink-0 items-center gap-1">
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
{/snippet}

<Modal
	{open}
	onclose={close}
	layout="bare"
	size="xl"
	height="fill"
	labelledby="shortcuts-title"
>
	<nav
		class="flex w-48 shrink-0 flex-col border-r border-line bg-panel px-2 py-3"
		aria-label="Shortcut groups"
	>
		<p
			id="shortcuts-title"
			class="mb-3 px-2 text-body-lg font-semibold tracking-tight text-fg"
		>
			Shortcuts
		</p>
		<div class="flex flex-col gap-0.5">
			{#each APP_SHORTCUT_GROUPS as group (group.id)}
				{@const active = !filtering && activeId === group.id}
				<button
					type="button"
					class={cn(
						'flex items-center gap-2 rounded-md border-none px-2 py-2 text-left text-label font-medium transition-colors duration-150',
						active
							? 'bg-accent-muted text-accent'
							: 'bg-transparent text-fg-secondary hover:bg-hover hover:text-fg',
					)}
					aria-current={active ? 'page' : undefined}
					onclick={() => {
						activeId = group.id
						query = ''
					}}
				>
					<Icon
						name={GROUP_ICONS[group.id] ?? 'help'}
						class={cn('size-3.5', active ? 'text-accent' : 'text-fg-muted')}
					/>
					{group.title}
				</button>
			{/each}
		</div>
	</nav>

	<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-elevated">
		<header
			class="flex shrink-0 items-start justify-between gap-3 border-b border-line px-5 py-3"
		>
			<div class="min-w-0">
				<h2 class="m-0 text-title font-semibold tracking-tight text-fg">
					{filtering ? 'Matches' : current.title}
				</h2>
				<p class="mt-1 mb-0 text-label text-fg-secondary">
					{visible.length}
					{visible.length === 1 ? 'shortcut' : 'shortcuts'}
				</p>
			</div>
			<IconButton
				label="Close shortcuts"
				variant="quiet"
				size="sm"
				onclick={close}
			>
				<Icon name="close" />
			</IconButton>
		</header>

		<div class="min-h-0 flex-1 overflow-auto px-5 py-4">
			<div class="relative mb-4">
				<label class="sr-only" for="shortcuts-filter">Filter shortcuts</label>
				<Icon
					name="search"
					class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-muted"
				/>
				<Input
					id="shortcuts-filter"
					type="search"
					placeholder="Filter shortcuts…"
					bind:value={query}
					inputSize="md"
					inset={query.length > 0 ? 'both' : 'start'}
					class="[appearance:textfield] [&::-webkit-search-cancel-button]:hidden [&::-webkit-search-decoration]:hidden"
					onkeydown={(e) => {
						if (e.key !== 'Escape' || !query) return
						e.stopPropagation()
						query = ''
					}}
				/>
				{#if query.length > 0}
					<button
						type="button"
						class="absolute top-1/2 right-1 grid size-6 -translate-y-1/2 place-items-center rounded-sm border-none bg-transparent text-fg-muted transition-colors duration-150 hover:bg-hover hover:text-fg focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1 enabled:active:scale-[0.97]"
						aria-label="Clear filter"
						onclick={() => (query = '')}
					>
						<Icon name="close" class="size-3" />
					</button>
				{/if}
			</div>

			{#if visible.length === 0}
				<p class="m-0 text-label text-fg-muted">
					Nothing matches “{query.trim()}”.
				</p>
			{:else}
				<ul
					class="m-0 list-none divide-y divide-line overflow-hidden rounded-lg border border-line bg-panel p-0"
					aria-label={filtering ? 'Matching shortcuts' : current.title}
				>
					{#each visible as row (row.item.id + row.group)}
						<li class="flex items-center justify-between gap-3 px-3 py-2.5">
							<span class="min-w-0">
								<span class="block truncate text-body text-fg">
									{#each matchParts(row.item.label, query) as part, i (i)}
										{#if part.hit}
											<mark class="rounded-sm bg-accent-muted p-0 text-fg"
												>{part.text}</mark
											>
										{:else}
											{part.text}
										{/if}
									{/each}
								</span>
								{#if filtering}
									<span class="mt-0.5 block text-caption text-fg-muted">
										{row.group}
									</span>
								{/if}
							</span>
							{@render chords(row.item)}
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	</div>
</Modal>
