<script lang="ts">
	import type { ConnectionProfile } from '$core/contracts'
	import { Button, Kbd } from '$core/ui'
	import {
		APP_SHORTCUT_GROUPS,
		isMacPlatform,
		labelForKey,
		type ShortcutItem,
	} from '$features/shortcuts'
	interface Props {
		profile: ConnectionProfile
		onnewquery?: () => void
	}

	let { profile, onnewquery }: Props = $props()

	const mac = isMacPlatform()

	const FEATURED_IDS = [
		'new-query',
		'close-tab',
		'save',
		'run',
		'format',
		'reset',
	] as const

	const featured = $derived.by((): ShortcutItem[] => {
		const byId = new Map(
			APP_SHORTCUT_GROUPS.flatMap((g) => g.items).map((item) => [
				item.id,
				item,
			]),
		)
		return FEATURED_IDS.map((id) => byId.get(id)).filter(
			(item): item is ShortcutItem => item != null,
		)
	})
</script>

<div
	class="flex flex-1 items-center justify-center overflow-auto bg-app-wash p-8"
>
	<div class="w-full max-w-panel-sm">
		<p class="m-0 text-center text-caption text-fg-muted">{profile.name}</p>
		<h1
			class="m-0 mt-2 text-center text-pretty text-title font-semibold tracking-tight text-fg"
		>
			No tab open
		</h1>
		<p
			class="mx-auto mt-2 mb-0 max-w-prose-ui text-center text-body leading-relaxed text-fg-secondary"
		>
			Double-click a table in the explorer, or open a query.
		</p>
		<div class="mt-5 flex justify-center">
			<Button variant="primary" size="sm" onclick={() => onnewquery?.()}>
				New query
			</Button>
		</div>

		<ul
			class="mt-8 mb-0 list-none overflow-hidden rounded-xl border border-line bg-elevated p-1 shadow-sm"
			aria-label="Important shortcuts"
		>
			{#each featured as item (item.id)}
				<li
					class="flex items-center justify-between gap-3 rounded-lg px-3 py-2.5"
				>
					<span class="min-w-0 text-label text-fg-secondary">{item.label}</span>
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
				</li>
			{/each}
		</ul>
	</div>
</div>
