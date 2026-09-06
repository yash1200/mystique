<script lang="ts">
	import { Icon, IconButton, Modal } from '$core/ui'
	import { appAboutUi } from '../about.svelte'
</script>

<Modal
	open={appAboutUi.open}
	onclose={() => appAboutUi.close()}
	layout="bare"
	size="sm"
	labelledby="about-title"
	class="max-w-sm!"
>
	<div class="flex items-start gap-4 px-5 pt-5 pb-5">
		<img
			src="/brand/mystique-mark.svg"
			alt=""
			width="64"
			height="64"
			class="size-16 shrink-0 rounded-lg shadow-sm"
			aria-hidden="true"
		/>

		<div class="min-w-0 flex-1">
			<div class="flex items-start justify-between gap-2">
				<div class="min-w-0">
					<h2
						id="about-title"
						class="m-0 text-title font-semibold tracking-tight text-fg"
					>
						Mystique
					</h2>
					<p
						class="mt-1 mb-0 font-mono text-caption tabular-nums text-fg-muted"
					>
						{appAboutUi.version ?? '…'}
					</p>
				</div>
				<IconButton
					label="Close"
					variant="quiet"
					size="sm"
					onclick={() => appAboutUi.close()}
				>
					<Icon name="close" />
				</IconButton>
			</div>

			<p class="mt-4 mb-0 text-label leading-relaxed text-fg-secondary">
				Desktop database client.
			</p>

			<p class="mt-4 mb-0 text-caption text-fg-muted">MIT</p>
		</div>
	</div>
</Modal>
