/**
 * Home screen: connection profiles, the engine picker, and the create/edit form.
 * The page itself (`ui/ConnectionsPage.svelte`) is not exported here: it
 * composes the workspace session and settings, so the home route mounts it
 * directly and the kernel can import this index without a cycle.
 */
export * from './api'
export {
	asColorId,
	asEnvironment,
	colorHex,
	configBool,
	configNum,
	configStr,
	CONNECTION_COLORS,
	connectionEndpoint,
	connectionInitial,
	draftFromProfile,
	namePlaceholder,
	pluginConfigForSession,
	pluginConfigFromDraft,
	saveInputFromDraft,
} from './model'
export {
	ConnectCancelled,
	isConnectCancelled,
	isPasswordRequired,
} from './password-prompt'
export {
	ensureSessionWithPassword,
	openSessionWithPassword,
	passwordPrompt,
	testSessionWithPassword,
} from './password-prompt.svelte'
export * from './profiles.svelte'
export { default as BrandMark } from './ui/BrandMark.svelte'
export { default as EngineBadge } from './ui/EngineBadge.svelte'
export { default as PasswordPromptModal } from './ui/PasswordPromptModal.svelte'
export { acceptSshHostKey, parseSshHostKey } from './ssh'
export { default as SshHostKeyModal } from './ui/SshHostKeyModal.svelte'
export { validateDraftForSave, validateDraftForTest } from './validate'
