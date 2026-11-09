import type { ToolAccessView } from '#core/contracts'

export type {
	McpHarness,
	McpStatus,
	ToolAccessConnection,
	ToolAccessDatabase,
} from '#core/contracts'

/** `none < read < write < full`, for MCP and the built-in agent. */
export type AccessLevel = 'none' | 'read' | 'write' | 'full'

export type ToolCaller = 'mcp' | 'agent'

/** `ToolAccessView` (generated). Levels are the four strings above. */
export type ToolAccess = ToolAccessView
