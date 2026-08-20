<script lang="ts">
	import type { CatalogObjectKind } from '$lib/types/workspace'
	import { cn } from '$lib/utils/cn'

	type IconName = CatalogObjectKind | 'schema' | 'database' | 'chevron'

	interface Props {
		name: IconName
		class?: string
		/** Active / selected accent */
		active?: boolean
	}

	let { name, class: className = '', active = false }: Props = $props()

	const tone = $derived(active ? 'text-accent' : 'text-fg-muted')
</script>

<svg
	class={cn('size-3.5 shrink-0', tone, className)}
	viewBox="0 0 16 16"
	fill="none"
	aria-hidden="true"
>
	{#if name === 'table'}
		<!-- Table grid -->
		<rect
			x="1.5"
			y="2.5"
			width="13"
			height="11"
			rx="1.5"
			stroke="currentColor"
			stroke-width="1.25"
		/>
		<path
			d="M1.5 6h13M1.5 9.5h13M6 6v7.5M10.5 6v7.5"
			stroke="currentColor"
			stroke-width="1.25"
		/>
		<path d="M1.5 2.5h13v3.5h-13z" fill="currentColor" opacity="0.18" />
	{:else if name === 'view'}
		<!-- Eye / view -->
		<path
			d="M1.75 8s2.25-3.75 6.25-3.75S14.25 8 14.25 8s-2.25 3.75-6.25 3.75S1.75 8 1.75 8z"
			stroke="currentColor"
			stroke-width="1.25"
			stroke-linejoin="round"
		/>
		<circle cx="8" cy="8" r="1.75" stroke="currentColor" stroke-width="1.25" />
	{:else if name === 'collection'}
		<!-- Stacked documents -->
		<rect
			x="3.5"
			y="3"
			width="9"
			height="10"
			rx="1.25"
			stroke="currentColor"
			stroke-width="1.25"
		/>
		<path
			d="M5.5 1.75h7.25A1.25 1.25 0 0 1 14 3v8.25"
			stroke="currentColor"
			stroke-width="1.25"
			stroke-linecap="round"
		/>
		<path
			d="M5.75 6.25h4.5M5.75 8.75h3"
			stroke="currentColor"
			stroke-width="1.25"
			stroke-linecap="round"
		/>
	{:else if name === 'schema'}
		<!-- Layers / schema folder -->
		<path
			d="M2 5.25 8 2.5l6 2.75-6 2.75L2 5.25z"
			stroke="currentColor"
			stroke-width="1.25"
			stroke-linejoin="round"
		/>
		<path
			d="M2 8.25 8 11l6-2.75"
			stroke="currentColor"
			stroke-width="1.25"
			stroke-linecap="round"
			stroke-linejoin="round"
		/>
		<path
			d="M2 11 8 13.75 14 11"
			stroke="currentColor"
			stroke-width="1.25"
			stroke-linecap="round"
			stroke-linejoin="round"
		/>
	{:else if name === 'database'}
		<!-- Cylinder -->
		<ellipse
			cx="8"
			cy="4"
			rx="5.25"
			ry="2"
			stroke="currentColor"
			stroke-width="1.25"
		/>
		<path
			d="M2.75 4v8c0 1.1 2.35 2 5.25 2s5.25-.9 5.25-2V4"
			stroke="currentColor"
			stroke-width="1.25"
		/>
		<path
			d="M2.75 8c0 1.1 2.35 2 5.25 2s5.25-.9 5.25-2"
			stroke="currentColor"
			stroke-width="1.25"
		/>
	{:else if name === 'chevron'}
		<path
			d="M6 4.5 9.5 8 6 11.5"
			stroke="currentColor"
			stroke-width="1.35"
			stroke-linecap="round"
			stroke-linejoin="round"
		/>
	{/if}
</svg>
