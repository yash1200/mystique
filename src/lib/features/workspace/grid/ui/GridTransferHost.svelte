<script lang="ts">
	import { resolveCapabilities } from '$engines'
	import { workspaceSession, workspaceStudio } from '$features/workspace/shell'
	import {
		closeImport,
		confirmExport,
		confirmImportFile,
		confirmImportRun,
		loadImportPreview,
	} from '../transfer.svelte'
	import ExportDataModal from './ExportDataModal.svelte'
	import ImportDataModal from './ImportDataModal.svelte'
	import ImportPreviewModal from './ImportPreviewModal.svelte'

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

<ImportDataModal
	open={!!workspaceStudio.importTable && !workspaceStudio.importPreview}
	{objectLabel}
	{formats}
	busy={workspaceStudio.importBusy}
	error={workspaceStudio.importError}
	onconfirm={(spec) => void confirmImportFile(spec)}
	oncancel={closeImport}
/>

<ImportPreviewModal
	open={!!workspaceStudio.importTable && !!workspaceStudio.importPreview}
	objectName={workspaceStudio.importTable?.name ?? ''}
	preview={workspaceStudio.importPreview}
	busy={workspaceStudio.importBusy}
	error={workspaceStudio.importError}
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
	onback={() => {
		if (workspaceStudio.importBusy) return
		workspaceStudio.importPreview = null
		workspaceStudio.importError = null
	}}
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
		if (workspaceStudio.exportBusy) return
		workspaceStudio.exportTable = null
		workspaceStudio.exportError = null
	}}
/>
