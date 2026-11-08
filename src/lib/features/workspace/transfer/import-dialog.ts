/**
 * Import is one dialog: pick the file, map the columns, run. On production
 * the run button says so; nothing else asks.
 */
export function importDialogCopy(input: {
	objectName: string
	production: boolean
	busy: boolean
	hasPreview: boolean
}): { title: string; confirm: string } {
	const title = input.production
		? `Import into production · ${input.objectName}`
		: `Import into ${input.objectName}`
	if (input.busy) {
		return { title, confirm: input.hasPreview ? 'Importing…' : 'Reading…' }
	}
	return {
		title,
		confirm: input.production ? 'Import into production' : 'Import',
	}
}
