<script lang="ts">
	import { isTauri } from '#core/ipc'
	import { Button, ControlGroup, ControlRow } from '#core/ui'
	import { showToast } from '#features/toast'
	import { onMount } from 'svelte'
	import { defaultDiagnosticsName, exportDiagnostics } from '../api'

	const desktop = isTauri()
	let version = $state<string | null>(null)
	let busy = $state(false)
	let error = $state<string | null>(null)

	/** The save dialog's path, `null` if cancelled, `undefined` without a dialog. */
	async function pickPath(): Promise<string | null | undefined> {
		try {
			const { save } = await import('@tauri-apps/plugin-dialog')
			return await save({
				title: 'Export diagnostics',
				defaultPath: defaultDiagnosticsName(),
				filters: [{ name: 'Diagnostics bundle', extensions: ['gz'] }],
			})
		} catch {
			return undefined
		}
	}

	async function exportBundle() {
		busy = true
		error = null
		try {
			const path = await pickPath()
			if (path === null) return
			const written = await exportDiagnostics(path)
			showToast('success', `Diagnostics saved to ${written}`)
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		} finally {
			busy = false
		}
	}

	onMount(() => {
		if (!desktop) return
		void import('@tauri-apps/api/app')
			.then(({ getVersion }) => getVersion())
			.then((v) => (version = v))
			.catch(() => {})
	})
</script>

{#if !desktop}
	<p class="m-0 text-caption text-fg-muted">
		Diagnostics are available in the desktop app.
	</p>
{:else}
	<div class="flex max-w-panel-md flex-col">
		<ControlGroup labelledby="settings-pane-title">
			<ControlRow label="Version">
				<span class="font-mono text-caption text-fg-muted">{version ?? '…'}</span>
			</ControlRow>
			<ControlRow
				label="Diagnostics"
				description="A .tar.gz to attach to a bug report: app version, OS, plugins, settings without secrets, connection flags and recent logs."
			>
				<Button size="sm" disabled={busy} onclick={exportBundle}>
					{busy ? 'Exporting…' : 'Export…'}
				</Button>
			</ControlRow>
		</ControlGroup>
		{#if error}
			<p class="mt-2 mb-0 text-caption text-danger" role="alert">{error}</p>
		{/if}
		<p class="mt-3 mb-0 max-w-prose-ui text-caption leading-relaxed text-fg-muted">
			The bundle never holds passwords, keys, tokens, host names, user names,
			SQL text or query history. Passwords in log lines are masked. Logs are
			in the app log folder; set <code>MYSTIQUE_LOG=debug</code> for more detail.
		</p>
	</div>
{/if}
