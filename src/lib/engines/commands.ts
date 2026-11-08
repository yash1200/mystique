import { defineCommand } from '#core/commands'

/** Open the "paste a connection URL" panel of the connection form. */
export const OPEN_URL_IMPORT = defineCommand({
	id: 'connections.open-url-import',
	title: 'Paste a connection URL',
})
