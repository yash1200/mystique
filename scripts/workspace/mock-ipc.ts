/**
 * Test helper: a fake Tauri backend. The app is desktop-only, so a test that
 * goes through `#core/ipc` answers each command here instead of mocking the
 * module. An unhandled command rejects, so a test never passes by accident.
 */
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { afterEach } from 'vitest'

export type IpcHandler = (args: Record<string, unknown>) => unknown

export type IpcCall = { command: string; args: Record<string, unknown> }

/** Answer `invoke(command, args)` from `handlers`. Returns the recorded calls. */
export function mockIpc(handlers: Record<string, IpcHandler>): IpcCall[] {
	const calls: IpcCall[] = []
	mockIPC((command, payload) => {
		const args = (payload ?? {}) as Record<string, unknown>
		calls.push({ command, args })
		const handler = handlers[command]
		if (!handler) throw new Error(`mock-ipc: no handler for ${command}`)
		return handler(args)
	})
	return calls
}

afterEach(() => {
	clearMocks()
})
