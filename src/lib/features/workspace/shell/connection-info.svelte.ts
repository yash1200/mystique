/**
 * Which connection the info modal shows. The session rail menu and the side
 * pane open it; WorkspaceShell mounts the one `ConnectionInfoModal`.
 */
export const connectionInfo = $state<{ id: string | null }>({ id: null })
