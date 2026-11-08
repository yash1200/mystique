import type { McpAccess as McpAccessRow } from '#core/contracts'

export type { McpAllowlistConnection, McpHarness, McpStatus } from '#core/contracts'

export type McpWritePolicy = 'read-only' | 'safe_write' | 'high_risk_write'

/** `McpAccess` (generated) with `policy` narrowed to the values the backend writes. */
export type McpAccess = Omit<McpAccessRow, 'policy'> & { policy: McpWritePolicy }
