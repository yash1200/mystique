<script lang="ts">
	import { resolveCapabilities } from '#engines'
	import { Button, Modal } from '#core/ui'
	import { hasProduction } from '#features/connections'
	import { workspaceSession } from '#features/workspace/session'
	import {
		answerImportGate,
		closeExport,
		closeImport,
		confirmExport,
		confirmImportFile,
		confirmImportRun,
		importGate,
		loadImportPreview,
		transferQueue,
		transferStore,
	} from '../transfer.svelte'
	import ExportDataModal from './ExportDataModal.svelte'
	import ImportDataModal from './ImportDataModal.svelte'
	import TransferQueuePanel from './TransferQueuePanel.svelte'

	// The dialog's own connection, not whichever one is active now.
	const activeSession = $derived(
		workspaceSession.sessions.find(
			(s) => s.profile.id === transferStore.connectionId,
		) ?? workspaceSession.activeSession,
	)
	const formats = $derived(
		activeSession
			? resolveCapabilities(
					String(activeSession.profile.engine),
					activeSession.capabilities,
				).exportFormats
			: ['csv', 'json', 'jsonl'],
	)
	const objectLabel = $derived(
		transferStore.importTable?.kind === 'collection' ||
			transferStore.exportTable?.kind === 'collection'
			? 'collection'
			: transferStore.exportTable?.kind === 'view'
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
	open={!!transferStore.importTable}
	connectionName={activeSession?.profile.name}
	objectName={transferStore.importTable?.name ?? ''}
	{objectLabel}
	{formats}
	production={!!activeSession && hasProduction(activeSession.profile)}
	preview={transferStore.importPreview}
	busy={transferStore.importBusy}
	error={transferStore.importError}
	onfile={(spec) => void confirmImportFile(spec)}
	onencoding={(enc) => {
		if (!transferStore.importTable || !transferStore.importPath) return
		void loadImportPreview(
			transferStore.importTable,
			transferStore.importPath,
			transferStore.importFormat,
			enc,
		)
	}}
	onconfirm={(spec) => void confirmImportRun(spec)}
	oncancel={closeImport}
/>

<ExportDataModal
	open={!!transferStore.exportTable}
	connectionName={activeSession?.profile.name}
	objectName={transferStore.exportTable?.name ?? ''}
	{objectLabel}
	{formats}
	busy={transferStore.exportBusy}
	error={transferStore.exportError}
	txnTab={transferStore.exportTxnTab}
	onconfirm={(spec) => void confirmExport(spec)}
	oncancel={closeExport}
/>

<TransferQueuePanel queue={transferQueue} />
