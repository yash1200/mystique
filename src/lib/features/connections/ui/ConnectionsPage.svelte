<script lang="ts">
	import { setNativeMenu } from '#core/ipc'
	import { Button, EmptyState, Icon, IconButton, Modal, Spinner } from '#core/ui'
	import { isMacPlatform } from '#core/util'
	import { appChrome } from '#lib/app'
	import { onMount } from 'svelte'
	import { HomeController, setHome } from '../home.svelte'
	import ConnectionDetail from './ConnectionDetail.svelte'
	import ConnectionSidebar from './ConnectionSidebar.svelte'
	import HomePeek from './HomePeek.svelte'
	import NewConnectionPanel, { FORM_ID } from './NewConnectionPanel.svelte'

	const home = new HomeController()
	setHome(home)

	const SAVE_KEYS = isMacPlatform() ? '⌘S' : 'Ctrl S'

	const ready = $derived(home.load.state === 'ready')
	/** The create or edit form is on screen (it owns Cancel / Save in the bar). */
	const formShown = $derived(ready && (home.editing || !home.selected))
	const createKey = $derived(home.view.kind === 'create' ? home.view.key : 0)
	const crumb = $derived(
		home.editing
			? `Edit ${home.selected?.name}`
			: (home.selected?.name ?? 'New connection'),
	)

	$effect(() => {
		setNativeMenu({
			page: 'home',
			hasSelection: home.selected != null,
			hasForm: formShown,
		})
	})

	onMount(home.mount)
</script>

<div class="flex h-screen w-screen overflow-hidden bg-app">
	{#if appChrome.homeSidebarOpen}
		<ConnectionSidebar />
	{/if}

	<main class="relative h-full min-w-0 flex-1 overflow-hidden">
		<div class="absolute inset-0 overflow-auto">
			<div
				class="sticky top-0 z-20 flex h-topbar items-center gap-2 border-b border-line bg-app-blur px-6"
			>
				{#if !appChrome.homeSidebarOpen}
					<IconButton
						label="Show sidebar"
						variant="quiet"
						size="sm"
						class="-ml-2"
						onclick={() => (appChrome.homeSidebarOpen = true)}
					>
						<Icon name="sidebar" />
					</IconButton>
				{/if}
				<nav class="flex min-w-0 items-center gap-2" aria-label="Breadcrumb">
					<span class="shrink-0 text-fg-muted">Connections</span>
					<Icon name="chevron-right" class="text-fg-muted" />
					<span class="min-w-0 truncate font-medium" aria-current="page">{crumb}</span>
				</nav>
				{#if formShown}
					<div class="ml-auto flex shrink-0 items-center gap-2">
						<Button
							variant="ghost"
							disabled={home.saving}
							onclick={home.editing ? home.cancelForm : home.create}
						>
							{home.editing ? 'Cancel' : 'Clear'}
							{#if home.editing}<kbd>esc</kbd>{/if}
						</Button>
						<Button
							type="submit"
							form={FORM_ID}
							variant="primary"
							disabled={home.saving || home.busy != null}
						>
							{#if home.saving}
								<Spinner size="sm" />Saving…
							{:else}
								{home.editing ? 'Update' : 'Save'}
								<kbd class="border-black/15 bg-black/10 text-current">{SAVE_KEYS}</kbd>
							{/if}
						</Button>
					</div>
				{/if}
			</div>

			{#if home.load.state === 'loading'}
				<div class="flex h-[70%] flex-col items-center justify-center gap-3 p-8" aria-busy="true">
					<Spinner class="text-fg-muted" />
					<EmptyState title="Loading connections…" />
				</div>
			{:else if home.load.state === 'error'}
				<div class="flex h-[70%] items-center justify-center p-8">
					<EmptyState title="Could not load connections" description={home.load.message} />
				</div>
			{:else if home.editing && home.selected}
				{#key home.selected.id}
					<NewConnectionPanel initial={home.selected} />
				{/key}
			{:else if home.selected}
				<ConnectionDetail connection={home.selected} />
			{:else}
				{#key createKey}
					<NewConnectionPanel />
				{/key}
			{/if}
		</div>

		{#if ready}
			<HomePeek />
		{/if}
	</main>

	<Modal
		open={!!home.pendingDelete}
		onclose={home.cancelDelete}
		title="Delete connection?"
		description={home.pendingDelete
			? `“${home.pendingDelete.name}” and its saved queries will be removed from this machine. This cannot be undone.`
			: undefined}
		dismissible={!home.deleting}
	>
		{#snippet footer()}
			<Button variant="ghost" onclick={home.cancelDelete} disabled={home.deleting}>
				Cancel
			</Button>
			<Button variant="danger" onclick={home.confirmDelete} disabled={home.deleting}>
				{#if home.deleting}
					<Spinner size="sm" />Deleting…
				{:else}
					<Icon name="trash" />Delete connection
				{/if}
			</Button>
		{/snippet}
	</Modal>
</div>
