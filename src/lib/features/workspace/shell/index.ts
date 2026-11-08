/**
 * Workspace chrome: panes, session rail, tabs, and modals. Composition only;
 * the state lives in `#features/workspace/session`. Only routes import this.
 */
export * from './shortcuts'
export { default as OpenConnectionModal } from './ui/OpenConnectionModal.svelte'
export { default as ResetConfirmModal } from './ui/ResetConfirmModal.svelte'
export { default as WorkspacePage } from './ui/WorkspacePage.svelte'
export { default as WorkspaceShell } from './ui/WorkspaceShell.svelte'
