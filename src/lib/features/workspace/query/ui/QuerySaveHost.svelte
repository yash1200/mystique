<script lang="ts">
	import { Button, Modal } from '#core/ui'
	import { workspaceStudio } from '#features/workspace/shell'
	import { queryLibrary } from '../library.svelte'
	import { querySave } from '../save.svelte'
	import SaveQueryModal from './SaveQueryModal.svelte'
</script>

<SaveQueryModal
	open={querySave.modalOpen}
	initialName={querySave.initialName}
	busy={workspaceStudio.querySaving}
	error={querySave.error}
	onconfirm={(name) => void querySave.confirmName(name)}
	oncancel={() => {
		if (workspaceStudio.querySaving) return
		querySave.thenClose = false
		querySave.closeModal(false)
	}}
/>

<SaveQueryModal
	open={queryLibrary.renameOpen}
	initialName={queryLibrary.renameInitial}
	busy={queryLibrary.renameBusy}
	error={queryLibrary.renameError}
	title="Rename query"
	description="Update the name for this saved query."
	confirmLabel="Rename"
	onconfirm={(name) => void queryLibrary.confirmRename(name)}
	oncancel={() => queryLibrary.cancelRename()}
/>

<Modal
	open={queryLibrary.deleteOpen}
	onclose={() => queryLibrary.cancelDelete()}
	title="Delete query"
	description="Delete “{queryLibrary.deleteName}”? This cannot be undone. Open tabs linked to this query will be closed."
	dismissible={!queryLibrary.deleteBusy}
>
	{#snippet footer()}
		<Button
			variant="ghost"
			size="sm"
			disabled={queryLibrary.deleteBusy}
			onclick={() => queryLibrary.cancelDelete()}
		>
			Cancel
		</Button>
		<Button
			variant="danger"
			size="sm"
			disabled={queryLibrary.deleteBusy}
			onclick={() => void queryLibrary.confirmDelete()}
		>
			{queryLibrary.deleteBusy ? 'Deleting…' : 'Delete'}
		</Button>
	{/snippet}
</Modal>
