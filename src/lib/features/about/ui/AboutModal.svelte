<script lang="ts">
	/**
	 * App menu › About Mystique. The mark, the version (click to copy for a
	 * bug report), and the project links. Diagnostics stay in
	 * Settings › About, one click away.
	 */
	import { Icon, IconButton, Modal } from '#core/ui'
	import { cn, copyTextToClipboard } from '#core/util'
	import { appSettingsUi } from '#features/settings'
	import { showToast } from '#features/toast'
	import {
		LICENSE_URL,
		openExternal,
		readAppVersion,
		releaseNotesUrl,
		REPO_URL,
	} from '../api'

	interface Props {
		open: boolean
		onclose: () => void
	}

	let { open, onclose }: Props = $props()

	const TITLE_ID = 'about-title'
	const COPIED_MS = 1600
	const SPIN_MS = 900
	const YEAR = new Date().getFullYear()

	let version = $state<string | null>(null)
	let copied = $state(false)
	let spinning = $state(false)
	let copiedTimer: ReturnType<typeof setTimeout> | undefined
	let spinTimer: ReturnType<typeof setTimeout> | undefined

	$effect(() => {
		if (!open || version) return
		void readAppVersion().then((v) => (version = v))
	})

	$effect(() => () => {
		clearTimeout(copiedTimer)
		clearTimeout(spinTimer)
	})

	async function copyVersion() {
		if (!version) return
		const result = await copyTextToClipboard(`Mystique ${version}`)
		if (!result.ok) {
			showToast('danger', 'Could not copy the version.')
			return
		}
		copied = true
		clearTimeout(copiedTimer)
		copiedTimer = setTimeout(() => (copied = false), COPIED_MS)
	}

	function spin() {
		spinning = false
		requestAnimationFrame(() => (spinning = true))
		clearTimeout(spinTimer)
		spinTimer = setTimeout(() => (spinning = false), SPIN_MS)
	}

	function openSettings() {
		onclose()
		appSettingsUi.show('about')
	}

	const links = $derived([
		{
			label: 'Release notes',
			url: version ? releaseNotesUrl(version) : `${REPO_URL}/releases`,
		},
		{ label: 'GitHub', url: REPO_URL },
	])
</script>

<Modal {open} {onclose} layout="bare" size="sm" labelledby={TITLE_ID}>
	<div class="relative flex w-full flex-col">
		<IconButton
			label="Close"
			variant="quiet"
			size="sm"
			class="absolute top-3 right-3 z-10"
			onclick={onclose}
		>
			<Icon name="close" />
		</IconButton>

		<section class="stagger flex flex-col items-center gap-4 px-6 pt-9 pb-6 text-center">
			<button
				type="button"
				class="mark flex rounded-xl border-none bg-transparent p-0"
				aria-label="Mystique"
				onclick={spin}
			>
				<img
					src="/brand/mystique-mark.svg"
					alt=""
					width="64"
					height="64"
					class={cn('size-16 rounded-xl shadow-md', spinning && 'spin')}
				/>
			</button>

			<div class="flex flex-col items-center gap-1">
				<h2 id={TITLE_ID} class="m-0 text-title font-semibold tracking-tight text-fg">
					Mystique
				</h2>
				<p class="m-0 max-w-prose-ui text-body text-balance text-fg-secondary">
					A calm, fast desktop client for your databases.
				</p>
			</div>

			<button
				type="button"
				class="group inline-flex h-control-sm items-center gap-2 rounded-full border border-line bg-surface px-3 font-mono text-caption text-fg-secondary transition-colors duration-150 enabled:hover:border-line-strong enabled:hover:text-fg"
				title="Copy version"
				disabled={!version}
				onclick={() => void copyVersion()}
			>
				<span class="select-text">
					{version ? `Version ${version}` : 'Version …'}
				</span>
				<span
					class={cn(
						'flex [&_svg]:size-3',
						copied ? 'text-accent-text' : 'text-fg-muted group-hover:text-fg-secondary',
					)}
					aria-hidden="true"
				>
					<Icon name={copied ? 'check' : 'copy'} />
				</span>
				<span class="sr-only" aria-live="polite">{copied ? 'Copied' : ''}</span>
			</button>
		</section>

		<div class="px-5 pb-4">
			<p class="m-0 text-center text-caption text-fg-muted">
				© {YEAR} Mystique contributors ·
				<button
					type="button"
					class="border-none bg-transparent p-0 text-caption text-fg-muted underline decoration-line-strong underline-offset-2 transition-colors duration-150 hover:text-fg"
					onclick={() => void openExternal(LICENSE_URL)}
				>
					MIT License
				</button>
			</p>
		</div>

		<footer
			class="flex flex-wrap items-center gap-x-1 gap-y-2 border-t border-line bg-surface px-3 py-2.5"
		>
			<nav class="flex min-w-0 flex-1 flex-wrap items-center gap-0.5" aria-label="Project links">
				{#each links as link (link.label)}
					<button
						type="button"
						class="inline-flex h-control-sm items-center gap-1 rounded-[7px] border-none bg-transparent px-2 text-label font-medium text-fg-secondary transition-colors duration-150 hover:bg-hover hover:text-fg [&_svg]:size-3"
						onclick={() => void openExternal(link.url)}
					>
						{link.label}
						<span class="flex text-fg-muted" aria-hidden="true"><Icon name="external" /></span>
					</button>
				{/each}
			</nav>
			<button
				type="button"
				class="inline-flex h-control-sm items-center gap-1.5 rounded-[7px] border-none bg-transparent px-2 text-label font-medium text-fg-secondary transition-colors duration-150 hover:bg-hover hover:text-fg [&_svg]:size-3.5"
				onclick={openSettings}
			>
				<span class="flex text-fg-muted" aria-hidden="true"><Icon name="settings" /></span>
				Diagnostics
			</button>
		</footer>
	</div>
</Modal>

<style>
	.mark:active img {
		transform: scale(0.97);
	}
	.mark img {
		transition: transform var(--duration-fast) var(--ease-standard);
	}
	.spin {
		animation: spin 0.9s var(--ease-emphasized);
	}
	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.spin {
			animation: none;
		}
	}
</style>
