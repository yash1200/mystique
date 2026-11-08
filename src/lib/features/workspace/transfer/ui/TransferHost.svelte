<script lang="ts">
	import { resolveCapabilities } from '#engines'
	import { Button, Modal } from '#core/ui'
	import { workspaceSession, workspaceStudio } from '#features/workspace/session'
	import {
		answerImportGate,
		cancelActiveExport,
		closeImport,
		confirmExport,
		confirmImportFile,
		confirmImportRun,
		importGate,
		loadImportPreview,
	} from '../transfer.svelte'
	import ExportDataModal from './ExportDataModal.svelte'
	import ImportDataModal from './ImportDataModal.svelte'

	const activeSession = $derived(workspaceSession.activeSession)
	const formats = $derived(
		activeSession
			? resolveCapabilities(
					String(activeSession.profile.engine),
					activeSession.capabilities,
				).exportFormats
			: ['csv', 'json', 'jsonl'],
	)
	const objectLabel = $derived(
		workspaceStudio.importTable?.kind === 'collection' ||
			workspaceStudio.exportTable?.kind === 'collection'
			? 'collection'
			: workspaceStudio.exportTable?.kind === 'view'
				? 'view'
				: 'table',
	)
</script>

<Modal
	open={!!importGate.table && !importGate.saving}
	onclose={() => void answerImportGate('cancel')}
	title="Unsaved edits"
	description="“{importGate.table?.name ?? ''}” has edits that are not saved. Import reloads the table. Save them first, discard them, or cancel the import."
>
	{#if importGate.error}
		<p class="text-caption text-danger" role="alert">{importGate.error}</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => void answerImportGate('cancel')}>
			Cancel
		</Button>
		<Button variant="ghost" size="sm" onclick={() => void answerImportGate('discard')}>
			Discard edits
		</Button>
		<Button variant="primary" size="sm" onclick={() => void answerImportGate('save')}>
			Save first
		</Button>
	{/snippet}
</Modal>

<ImportDataModal
	open={!!workspaceStudio.importTable}
	objectName={workspaceStudio.importTable?.name ?? ''}
	{objectLabel}
	{formats}
	production={activeSession?.profile.environment === 'production'}
	preview={workspaceStudio.importPreview}
	busy={workspaceStudio.importBusy}
	error={workspaceStudio.importError}
	onfile={(spec) => void confirmImportFile(spec)}
	onencoding={(enc) => {
		if (!workspaceStudio.importTable || !workspaceStudio.importPath) return
		void loadImportPreview(
			workspaceStudio.importTable,
			workspaceStudio.importPath,
			workspaceStudio.importFormat,
			enc,
		)
	}}
	onconfirm={(spec) => void confirmImportRun(spec)}
	oncancel={closeImport}
/>

<ExportDataModal
	open={!!workspaceStudio.exportTable}
	objectName={workspaceStudio.exportTable?.name ?? ''}
	{objectLabel}
	{formats}
	busy={workspaceStudio.exportBusy}
	error={workspaceStudio.exportError}
	onconfirm={(spec) => void confirmExport(spec)}
	oncancel={() => {
		if (workspaceStudio.exportBusy) {
			cancelActiveExport()
			return
		}
		workspaceStudio.exportTable = null
		workspaceStudio.exportError = null
	}}
/>
