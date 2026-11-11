<script lang="ts">
	import { Icon, IconButton, Switch } from '#core/ui'

	interface Props {
		live: boolean
		loading: boolean
		canRefresh: boolean
		onlive: (live: boolean) => void
		onrefresh: () => void
	}

	let { live, loading, canRefresh, onlive, onrefresh }: Props = $props()
</script>

<header
	class="flex shrink-0 flex-wrap items-center gap-2 border-b border-line bg-surface px-3 py-2"
>
	<div class="flex min-w-0 flex-1 items-center gap-2">
		<Icon name="metrics" class="size-5 text-info" />
		<h1 class="m-0 text-title font-semibold tracking-tight text-fg">
			Server metrics
		</h1>
	</div>
	<div class="flex flex-wrap items-center gap-2">
		<Switch checked={live} ariaLabel="Live refresh" onchange={onlive} />
		<span class="text-caption text-fg-secondary">Live</span>
		<IconButton
			label="Refresh metrics"
			variant="quiet"
			size="sm"
			disabled={loading || !canRefresh}
			onclick={onrefresh}
		>
			<Icon name="refresh" />
		</IconButton>
	</div>
</header>
