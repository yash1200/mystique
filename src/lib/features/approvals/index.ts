/**
 * In-app approval dock for agent and MCP writes above their access level
 * ("Ask me for high-risk writes"). Mount `ApprovalDock` once.
 */
export * from './api'
export { approvalQueue } from './queue.svelte'
export { default as ApprovalDock } from './ui/ApprovalDock.svelte'
